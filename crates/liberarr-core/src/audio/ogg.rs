//! Ogg page integrity check: capture pattern, CRC and page sequence continuity.

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum OggError {
    #[error("empty Ogg stream")]
    Empty,
    #[error("page {0}: bad capture pattern, version or truncated")]
    BadPage(usize),
    #[error("page {0}: CRC mismatch")]
    BadCrc(usize),
    #[error("page {0}: sequence number not contiguous")]
    BadSequence(usize),
}

const fn crc_table() -> [u32; 256] {
    let mut t = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = (i as u32) << 24;
        let mut k = 0;
        while k < 8 {
            c = if c & 0x8000_0000 != 0 {
                (c << 1) ^ 0x04C1_1DB7
            } else {
                c << 1
            };
            k += 1;
        }
        t[i] = c;
        i += 1;
    }
    t
}

static CRC_TABLE: [u32; 256] = crc_table();

/// CRC of a page with its own CRC field (bytes 22..26) read as zero.
fn page_crc(page: &[u8]) -> u32 {
    page.iter().enumerate().fold(0u32, |c, (i, &b)| {
        let b = if (22..26).contains(&i) { 0 } else { b };
        (c << 8) ^ CRC_TABLE[((c >> 24) as u8 ^ b) as usize]
    })
}

/// Every page must start with `OggS`, carry a valid CRC and follow the previous page's sequence
/// number; the data must end exactly on a page boundary. Assumes one logical stream.
pub fn verify_ogg(data: &[u8]) -> Result<(), OggError> {
    if data.is_empty() {
        return Err(OggError::Empty);
    }
    let (mut rest, mut prev, mut n) = (data, None::<u32>, 0usize);
    while !rest.is_empty() {
        let bad = OggError::BadPage(n);
        let head = rest
            .get(..27)
            .filter(|h| &h[..4] == b"OggS" && h[4] == 0)
            .ok_or(bad)?;
        let segs = head[26] as usize;
        let body: usize = rest
            .get(27..27 + segs)
            .ok_or(OggError::BadPage(n))?
            .iter()
            .map(|&s| s as usize)
            .sum();
        let (page, tail) = rest
            .split_at_checked(27 + segs + body)
            .ok_or(OggError::BadPage(n))?;
        if u32::from_le_bytes([page[22], page[23], page[24], page[25]]) != page_crc(page) {
            return Err(OggError::BadCrc(n));
        }
        let seq = u32::from_le_bytes([page[18], page[19], page[20], page[21]]);
        if prev.is_some_and(|p| seq != p.wrapping_add(1)) {
            return Err(OggError::BadSequence(n));
        }
        (rest, prev, n) = (tail, Some(seq), n + 1);
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Independent bitwise CRC (poly 0x04C11DB7, MSB first, init 0, no final xor) to build pages.
    fn crc(bytes: &[u8]) -> u32 {
        let mut c = 0u32;
        for &b in bytes {
            c ^= (b as u32) << 24;
            for _ in 0..8 {
                c = if c & 0x8000_0000 != 0 {
                    (c << 1) ^ 0x04C1_1DB7
                } else {
                    c << 1
                };
            }
        }
        c
    }

    pub(crate) fn page(seq: u32, body: &[u8]) -> Vec<u8> {
        let mut p = b"OggS".to_vec();
        p.extend_from_slice(&[0, if seq == 0 { 2 } else { 0 }]);
        p.extend_from_slice(&[0; 8]); // granule
        p.extend_from_slice(&1234u32.to_le_bytes()); // stream serial
        p.extend_from_slice(&seq.to_le_bytes());
        p.extend_from_slice(&[0; 4]); // crc placeholder
        p.push(1);
        p.push(body.len() as u8);
        p.extend_from_slice(body);
        let c = crc(&p);
        p[22..26].copy_from_slice(&c.to_le_bytes());
        p
    }

    fn stream() -> Vec<u8> {
        [page(0, b"head"), page(1, b"first body"), page(2, b"last")].concat()
    }

    #[test]
    fn verify_ogg_detects_flipped_byte() {
        let ok = stream();
        assert_eq!(verify_ogg(&ok), Ok(()));

        // Flip one byte anywhere: always rejected.
        for i in 0..ok.len() {
            let mut bad = ok.clone();
            bad[i] ^= 0x01;
            assert!(verify_ogg(&bad).is_err(), "flip at {i} accepted");
        }

        // Cut mid-page, trailing junk, empty input, and a sequence gap.
        assert!(verify_ogg(&ok[..ok.len() - 1]).is_err());
        assert!(verify_ogg(&[ok.clone(), vec![0; 3]].concat()).is_err());
        assert_eq!(verify_ogg(&[]), Err(OggError::Empty));
        let gap = [page(0, b"a"), page(2, b"b")].concat();
        assert_eq!(verify_ogg(&gap), Err(OggError::BadSequence(1)));
        // Truncation off a page boundary is an error, never a panic.
        let bounds = [
            page(0, b"head").len(),
            page(0, b"head").len() + page(1, b"first body").len(),
        ];
        for n in (0..ok.len()).filter(|n| !bounds.contains(n)) {
            assert!(verify_ogg(&ok[..n]).is_err(), "cut at {n} accepted");
        }
    }
}
