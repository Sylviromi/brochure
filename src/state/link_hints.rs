//! Keyboard link hints: `f` labels every link and image in the article on screen so typing a
//! label clicks it; `F` copies it instead.

use crate::models::HitAreas;
use ratatui::layout::Rect;

/// Home row first, so most labels are typed without leaving it. No `f`: a held `f`
/// repeats, and the repeats must not pick a label.
const ALPHABET: &[u8] = b"asdghjklqwertyuiopzxcvbnm";

/// An active hint session.
#[derive(Debug)]
pub struct LinkHints {
    /// `F`: copy the target instead of clicking it.
    pub copy: bool,
    /// Label letters typed so far.
    pub typed: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HintTarget<'a> {
    Link(&'a str),
    Image(&'a str),
}

/// Links and images drawn in the last frame, in reading order, each with its label.
///
/// A link wrapped over several rows gets one label, on its first row.
pub fn hint_targets(hit: &HitAreas) -> Vec<(String, Rect, HintTarget<'_>)> {
    let mut targets: Vec<(Rect, HintTarget)> = Vec::new();
    for (rect, url) in &hit.article_links {
        if !matches!(targets.last(), Some((_, HintTarget::Link(prev))) if prev == url) {
            targets.push((*rect, HintTarget::Link(url)));
        }
    }
    targets.extend(
        hit.article_images
            .iter()
            .filter(|(r, _)| !r.is_empty())
            .map(|(r, url)| (*r, HintTarget::Image(url))),
    );
    targets.sort_by_key(|(r, _)| (r.y, r.x));
    labels(targets.len())
        .into_iter()
        .zip(targets)
        .map(|(label, (rect, target))| (label, rect, target))
        .collect()
}

/// `n` prefix-free labels: single letters while they suffice, else two letters each.
fn labels(n: usize) -> Vec<String> {
    let letter = |i: usize| char::from(ALPHABET[i % ALPHABET.len()]);
    if n <= ALPHABET.len() {
        (0..n).map(|i| letter(i).to_string()).collect()
    } else {
        // shortcut: caps at 625 targets, far more than a screen can show.
        (0..n.min(ALPHABET.len().pow(2)))
            .map(|i| format!("{}{}", letter(i / ALPHABET.len()), letter(i)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_merge_wrapped_links_and_sort_by_position() {
        let hit = HitAreas {
            article_links: vec![
                (Rect::new(10, 2, 5, 1), "https://a".into()),
                (Rect::new(0, 3, 4, 1), "https://a".into()),
                (Rect::new(3, 9, 4, 1), "https://b".into()),
            ],
            article_images: vec![(Rect::new(0, 5, 20, 3), "https://img".into())],
            ..Default::default()
        };
        let got: Vec<_> = hint_targets(&hit)
            .into_iter()
            .map(|(l, _, t)| (l, t))
            .collect();
        assert_eq!(
            got,
            [
                ("a".into(), HintTarget::Link("https://a")),
                ("s".into(), HintTarget::Image("https://img")),
                ("d".into(), HintTarget::Link("https://b")),
            ]
        );
    }
}
