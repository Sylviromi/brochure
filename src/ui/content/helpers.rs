//! Domain helpers: article context resolution and archived/current article grouping.

use crate::models::Article;

/// Splits articles into current and archived groups, returning indices.
/// Returns (current_indices, archived_indices, has_archived).
pub(crate) fn split_articles(articles: &[Article]) -> (Vec<usize>, Vec<usize>, bool) {
    let mut current = Vec::new();
    let mut archived = Vec::new();
    for (i, article) in articles.iter().enumerate() {
        if article.is_archived {
            archived.push(i);
        } else {
            current.push(i);
        }
    }
    let has_archived = !archived.is_empty();
    (current, archived, has_archived)
}
