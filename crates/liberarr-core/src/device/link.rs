//! Link use case: one-time fetch of the unlock material from the user's Plaud account (spec §6.2).
use super::ports::*;
use crate::{Error, Result};
use std::collections::HashMap;

/// The account calls of `link`. The token is used once and never stored.
pub trait PlaudAccount {
    /// Serials of the devices bound to the account.
    async fn devices(&self, token: &str) -> Result<Vec<String>>;
    /// Signature of each serial.
    async fn sign(&self, token: &str, serials: &[String]) -> Result<HashMap<String, Vec<u8>>>;
    /// `(private_pem, public_pem)`. Plaud keeps a copy, so this is called as rarely as possible.
    async fn key_pair(&self, token: &str) -> Result<(String, String)>;
    async fn user_id(&self, token: &str) -> Result<String>;
}

const SCAN_SECS: u64 = 12;

#[derive(Debug, PartialEq)]
pub enum Unlock {
    Ok,
    Failed(String),
    /// Keys are saved, but no known device was advertising.
    NotSeen,
}

#[derive(Debug, PartialEq)]
pub struct LinkReport {
    pub devices: usize,
    pub key_pair_fetched: bool,
    pub unlock: Unlock,
}

pub struct LinkDevice<'a, A, K, Sc, Co> {
    pub account: &'a A,
    pub keys: &'a K,
    pub scanner: &'a Sc,
    pub connector: &'a Co,
}

impl<A: PlaudAccount, K: KeyStore, Sc: DeviceScanner, Co: DeviceConnector>
    LinkDevice<'_, A, K, Sc, Co>
{
    pub async fn run(&self, token: &str, force: bool) -> Result<LinkReport> {
        let stored = self.keys.load()?;
        let serials = self.account.devices(token).await?;
        if serials.is_empty() {
            // Saving nothing would wipe working keys.
            return Err(Error::Device("no device on this Plaud account".into()));
        }
        let mut signatures = self.account.sign(token, &serials).await?;
        let bind_token = self.account.user_id(token).await?;
        // Plaud keeps a copy of every pair it generates: only ask when there is none (or forced).
        let (pair, key_pair_fetched) = match stored.first() {
            Some(m) if !force => ((m.private_pem.clone(), m.public_pem.clone()), false),
            _ => (self.account.key_pair(token).await?, true),
        };
        let materials = serials
            .into_iter()
            .map(|serial| {
                let signature = signatures
                    .remove(&serial)
                    .ok_or_else(|| Error::Device("a device serial was not signed".into()))?;
                Ok(UnlockMaterial {
                    serial,
                    signature,
                    private_pem: pair.0.clone(),
                    public_pem: pair.1.clone(),
                    bind_token: bind_token.clone(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        self.keys.save(&materials)?;

        let seen = self.scanner.scan(SCAN_SECS).await?;
        let target = seen.iter().find_map(|ad| {
            let m = materials
                .iter()
                .find(|m| m.serial.eq_ignore_ascii_case(&ad.serial))?;
            Some((ad, m))
        });
        let unlock = match target {
            None => Unlock::NotSeen,
            Some((ad, m)) => match self.connector.open(ad, m).await {
                Ok(_) => Unlock::Ok,
                Err(e) => Unlock::Failed(e.to_string()),
            },
        };
        Ok(LinkReport {
            devices: materials.len(),
            key_pair_fetched,
            unlock,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::fakes::*;
    use crate::fakes::block_on;
    use std::cell::Cell;

    #[derive(Default)]
    struct FakeAccount {
        key_pair_calls: Cell<u32>,
    }
    impl PlaudAccount for FakeAccount {
        async fn devices(&self, _: &str) -> Result<Vec<String>> {
            Ok(vec![SERIAL.into()])
        }
        async fn sign(&self, _: &str, serials: &[String]) -> Result<HashMap<String, Vec<u8>>> {
            Ok(serials.iter().map(|s| (s.clone(), vec![2; 256])).collect())
        }
        async fn key_pair(&self, _: &str) -> Result<(String, String)> {
            self.key_pair_calls.set(self.key_pair_calls.get() + 1);
            Ok(("new-private".into(), "new-public".into()))
        }
        async fn user_id(&self, _: &str) -> Result<String> {
            Ok("uid".into())
        }
    }

    #[test]
    fn link_does_not_call_key_pair_when_keys_exist() {
        let (account, keys, dev) = (
            FakeAccount::default(),
            MemKeys::default(),
            FakeDevice::default(),
        );
        let existing = material();
        keys.0.borrow_mut().push(existing.clone());
        let link = LinkDevice {
            account: &account,
            keys: &keys,
            scanner: &dev,
            connector: &dev,
        };

        let r = block_on(link.run("tok", false)).unwrap();
        assert_eq!(account.key_pair_calls.get(), 0);
        assert!(!r.key_pair_fetched);
        assert_eq!(r.unlock, Unlock::NotSeen);
        // The stored pair is kept; the signature is refreshed.
        let saved = keys.0.borrow()[0].clone();
        assert_eq!(saved.private_pem, existing.private_pem);
        assert_eq!(saved.signature, vec![2; 256]);

        // A device advertising gets a test unlock.
        dev.0.adverts.borrow_mut().push(advert(SERIAL));
        assert_eq!(block_on(link.run("tok", false)).unwrap().unlock, Unlock::Ok);
        assert_eq!(dev.0.opened.get(), 1);

        // Only --force replaces the pair, and an empty store has to fetch one.
        let r = block_on(link.run("tok", true)).unwrap();
        assert!(r.key_pair_fetched);
        assert_eq!(keys.0.borrow()[0].private_pem, "new-private");
        keys.0.borrow_mut().clear();
        block_on(link.run("tok", false)).unwrap();
        assert_eq!(account.key_pair_calls.get(), 2);
    }
}
