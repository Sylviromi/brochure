//! Flattening of the category/feed tree into visible sidebar and editor rows.

use crate::models::{Category, CategoryId, FAVORITES_URL, Feed, FeedTreeItem};
use std::collections::HashSet;

/// Compute the flattened, visible rows of the feed/category tree.
///
/// - Feeds with `url == FAVORITES_URL` are always excluded.
/// - Categories with their id in `collapsed` hide their children.
/// - Categories are sorted by `category.order`, feeds within each category by `feed.order`.
/// - Uncategorized feeds appear after all categories.
pub fn visible_tree_items(
    categories: &[Category],
    feeds: &[Feed],
    collapsed: &HashSet<CategoryId>,
) -> Vec<FeedTreeItem> {
    let mut result = Vec::new();
    collect_tree_level(categories, feeds, collapsed, None, 0, &mut result);
    result.extend(
        sorted_feeds_in(feeds, None)
            .into_iter()
            .map(|feeds_idx| FeedTreeItem::Feed {
                feeds_idx,
                depth: 0,
            }),
    );
    result
}

/// Like [`visible_tree_items`] but prepends the virtual `AllFeeds` entry at index 0.
///
/// Use this for the main sidebar; use [`visible_tree_items`] for the feed editor and other
/// contexts where `AllFeeds` should not appear.
pub fn sidebar_tree_items(
    categories: &[Category],
    feeds: &[Feed],
    collapsed: &HashSet<CategoryId>,
) -> Vec<FeedTreeItem> {
    let mut items = vec![FeedTreeItem::AllFeeds];
    items.extend(visible_tree_items(categories, feeds, collapsed));
    items
}

/// Returns only `FeedTreeItem::Category` rows from the visible tree.
/// Used by the left (categories-only) panel of the split editor.
pub fn visible_cat_only_items(
    categories: &[Category],
    feeds: &[Feed],
    collapsed: &HashSet<CategoryId>,
) -> Vec<FeedTreeItem> {
    visible_tree_items(categories, feeds, collapsed)
        .into_iter()
        .filter(|item| matches!(item, FeedTreeItem::Category { .. }))
        .collect()
}

/// Indices of the rows in `items` that are feeds (categories are headers).
pub(crate) fn feed_row_indices(items: &[FeedTreeItem]) -> Vec<usize> {
    items
        .iter()
        .enumerate()
        .filter(|(_, item)| matches!(item, FeedTreeItem::Feed { .. }))
        .map(|(i, _)| i)
        .collect()
}

/// Collects feed indices for all feeds directly or transitively under `cat_id`,
/// in tree order (sorted by order field within each level).
pub(crate) fn feeds_in_category(
    cat_id: CategoryId,
    categories: &[Category],
    feeds: &[Feed],
) -> Vec<usize> {
    let mut result = sorted_feeds_in(feeds, Some(cat_id));
    for subcat in sorted_children(categories, Some(cat_id)) {
        result.extend(feeds_in_category(subcat.id, categories, feeds));
    }
    result
}

/// Child categories of `parent`, sorted by display order.
fn sorted_children(categories: &[Category], parent: Option<CategoryId>) -> Vec<&Category> {
    let mut cats: Vec<&Category> = categories
        .iter()
        .filter(|c| c.parent_id == parent)
        .collect();
    cats.sort_by_key(|c| c.order);
    cats
}

/// Indices of the (non-Favorites) feeds directly in `category`, sorted by display order.
fn sorted_feeds_in(feeds: &[Feed], category: Option<CategoryId>) -> Vec<usize> {
    let mut direct: Vec<usize> = feeds
        .iter()
        .enumerate()
        .filter(|(_, f)| f.url != FAVORITES_URL && f.category_id == category)
        .map(|(i, _)| i)
        .collect();
    direct.sort_by_key(|&i| feeds[i].order);
    direct
}

/// Recursively append one depth level of the category/feed tree to `result`.
/// Children of a collapsed category are skipped.
fn collect_tree_level(
    categories: &[Category],
    feeds: &[Feed],
    collapsed: &HashSet<CategoryId>,
    parent_id: Option<CategoryId>,
    depth: u8,
    result: &mut Vec<FeedTreeItem>,
) {
    for cat in sorted_children(categories, parent_id) {
        let is_collapsed = collapsed.contains(&cat.id);
        result.push(FeedTreeItem::Category {
            id: cat.id,
            depth,
            collapsed: is_collapsed,
        });
        if is_collapsed {
            continue;
        }
        collect_tree_level(
            categories,
            feeds,
            collapsed,
            Some(cat.id),
            depth + 1,
            result,
        );
        result.extend(
            sorted_feeds_in(feeds, Some(cat.id))
                .into_iter()
                .map(|feeds_idx| FeedTreeItem::Feed {
                    feeds_idx,
                    depth: depth + 1,
                }),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_visible_tree_items_excludes_favorites() {
        let cats = vec![];
        let feeds = vec![
            Feed::new_local("⭐ Favorites".into(), FAVORITES_URL.into(), None, 0),
            Feed::new_local(
                "News".into(),
                "https://news.example.com/feed".into(),
                None,
                0,
            ),
        ];
        let items = visible_tree_items(&cats, &feeds, &HashSet::new());
        // Only "News" visible; Favorites excluded by URL check
        assert_eq!(items.len(), 1);
        assert!(matches!(items[0], FeedTreeItem::Feed { feeds_idx: 1, .. }));
    }

    #[test]
    fn test_visible_tree_items_collapsed_hides_children() {
        let cats = vec![Category {
            id: 1,
            name: "Tech".into(),
            parent_id: None,
            order: 0,
        }];
        let feeds = vec![
            Feed::new_local("⭐ Favorites".into(), FAVORITES_URL.into(), None, 0),
            Feed::new_local(
                "HN".into(),
                "https://news.ycombinator.com/rss".into(),
                Some(1),
                0,
            ),
        ];
        let collapsed = HashSet::from([1u64]);
        let items = visible_tree_items(&cats, &feeds, &collapsed);
        // Only the category header; feed hidden
        assert_eq!(items.len(), 1);
        assert!(matches!(
            items[0],
            FeedTreeItem::Category {
                id: 1,
                collapsed: true,
                ..
            }
        ));
    }

    #[test]
    fn nested_categories_list_subcategories_before_feeds() {
        let cats = vec![
            Category {
                id: 1,
                name: "Root".into(),
                parent_id: None,
                order: 0,
            },
            Category {
                id: 2,
                name: "Child".into(),
                parent_id: Some(1),
                order: 0,
            },
        ];
        let feeds = vec![
            Feed::new_local("a".into(), "a".into(), Some(1), 0),
            Feed::new_local("b".into(), "b".into(), Some(2), 0),
        ];
        let items = visible_tree_items(&cats, &feeds, &HashSet::new());
        assert!(matches!(
            items[0],
            FeedTreeItem::Category {
                id: 1,
                depth: 0,
                ..
            }
        ));
        assert!(matches!(
            items[1],
            FeedTreeItem::Category {
                id: 2,
                depth: 1,
                ..
            }
        ));
        assert!(matches!(
            items[2],
            FeedTreeItem::Feed {
                feeds_idx: 1,
                depth: 2
            }
        ));
        assert!(matches!(
            items[3],
            FeedTreeItem::Feed {
                feeds_idx: 0,
                depth: 1
            }
        ));
        assert_eq!(feeds_in_category(1, &cats, &feeds), vec![0, 1]);
    }
}
