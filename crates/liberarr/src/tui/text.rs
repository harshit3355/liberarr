//! Turns stored summary text into styled lines. The text comes from an LLM (untrusted): any input
//! must render, never panic.
use super::theme;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

/// Markdown subset of the summaries: `#` headings, `-`/`*` bullets, `**bold**`.
pub fn markdown(md: &str) -> Vec<Line<'static>> {
    md.lines()
        .map(|l| {
            let t = l.trim_start();
            if let Some(h) = t.strip_prefix('#') {
                Line::styled(
                    h.trim_start_matches('#').trim().to_string(),
                    theme::accent(),
                )
            } else if let Some(item) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
                let indent = " ".repeat(l.len() - t.len());
                let mut spans = vec![Span::raw(format!("{indent}• "))];
                spans.extend(bold(item));
                Line::from(spans)
            } else {
                Line::from(bold(l))
            }
        })
        .collect()
}

/// Splits on `**`: odd parts are bold. An unclosed `**` leaves the rest bold, harmless.
fn bold(s: &str) -> Vec<Span<'static>> {
    s.split("**")
        .enumerate()
        .filter(|(_, p)| !p.is_empty())
        .map(|(i, p)| {
            let style = if i % 2 == 1 {
                Style::new().add_modifier(Modifier::BOLD)
            } else {
                Style::new()
            };
            Span::styled(p.to_string(), style)
        })
        .collect()
}

/// A Mermaid `mindmap` as an indented tree: node shapes (`((…))`, `[…]`, `(…)`, `{{…}}`) are
/// dropped, the indentation gives the depth.
pub fn mindmap(src: &str) -> Vec<Line<'static>> {
    let nodes: Vec<(usize, String)> = src
        .lines()
        .filter(|l| !l.trim().is_empty() && l.trim() != "mindmap" && !l.trim().starts_with("%%"))
        .map(|l| (l.len() - l.trim_start().len(), label(l.trim())))
        .collect();
    let base = nodes.iter().map(|(i, _)| *i).min().unwrap_or(0);
    // Indent steps are not fixed: depth is the rank of the indentation among the ones seen.
    let mut levels: Vec<usize> = nodes.iter().map(|(i, _)| *i).collect();
    levels.sort_unstable();
    levels.dedup();
    nodes
        .into_iter()
        .map(|(indent, text)| {
            let depth = levels.iter().position(|l| *l == indent).unwrap_or(0);
            if indent == base {
                Line::styled(text, theme::accent())
            } else {
                Line::from(vec![
                    Span::styled(format!("{}└ ", "  ".repeat(depth - 1)), theme::dim()),
                    Span::raw(text),
                ])
            }
        })
        .collect()
}

/// `root((Title))` → `Title`, `id[Text]` → `Text`, `Text` → `Text`.
fn label(node: &str) -> String {
    let Some(open) = node.find(['(', '[', '{']) else {
        return node.to_string();
    };
    let inner = node[open..]
        .trim_start_matches(['(', '[', '{'])
        .trim_end_matches([')', ']', '}']);
    if inner.is_empty() {
        node.to_string()
    } else {
        inner.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(lines: &[Line]) -> Vec<String> {
        lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect()
    }

    #[test]
    fn mindmap_drops_shapes_and_keeps_depth() {
        let src = "mindmap\n  root((Liberarr))\n    Onboarding\n      id[TUI]\n    Providers";
        assert_eq!(
            plain(&mindmap(src)),
            ["Liberarr", "└ Onboarding", "  └ TUI", "└ Providers"]
        );
        // Untrusted text: odd shapes and empty input render without panicking.
        for odd in ["", "mindmap", "((", "  )]", "a\n\tb\n c", "é((ü))"] {
            let _ = mindmap(odd);
            let _ = markdown(odd);
        }
    }
}
