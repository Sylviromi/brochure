//! Article detail view rendering with markdown content, scrolling, and header.
//!
//! The header (title, date, description, hero image) and body are combined into a
//! single markdown document so that everything scrolls together.

use crate::{app::App, handlers::article::get_selected_article, models::CONTENT_STUB_MAX_LEN};
use limner::{
    Alignment, MarkdownStyle,
    render_image::{ImagePlacement, SignedPosition, SlicedImage, prepare_inline_images},
    render_markdown,
};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    symbols::{border, line},
    widgets::{Paragraph, Wrap},
};

use super::super::{content_block, render_scrollbar};
use super::{footer::draw_article_info, utils::format_pub_date};
use tui_skeleton::{AnimationMode, SkeletonBlock};

/// Base limner style config from the app theme (shared between header and body).
fn base_md_style(theme: &crate::ui::theme::ColorTheme) -> MarkdownStyle {
    MarkdownStyle {
        paragraph: Style::new().fg(theme.text),
        paragraph_alignment: Alignment::Left,
        heading_1: Style::new().fg(theme.accent).bold(),
        heading_1_alignment: Alignment::Left,
        heading_2: Style::new().fg(theme.accent).bold(),
        heading_2_alignment: Alignment::Left,
        heading_3: Style::new().fg(theme.accent),
        heading_3_alignment: Alignment::Left,
        bold: Style::new().bold(),
        italic: Style::new().italic(),
        strikethrough: Style::new().crossed_out(),
        inline_code: Style::new().fg(theme.code).bg(theme.code_bg),
        code_block: Style::new().fg(theme.code),
        code_block_bg: theme.bg_dark,
        code_block_border: Some(border::ROUNDED),
        code_block_border_style: Style::new().fg(theme.border),
        code_block_language: Style::new().fg(theme.accent).bold(),
        syntax_theme: Some(theme.syntax_theme()),
        table_border: line::ROUNDED,
        table_border_style: Style::new().fg(theme.border),
        table_header: Style::new().fg(theme.accent).bold(),
        link: Style::new().fg(theme.link).underlined(),
        link_prefix: "",
        quote: Style::new().fg(theme.muted),
        quote_alignment: Alignment::Left,
        quote_indicator: "▍ ",
        image: Style::new()
            .fg(theme.muted)
            .add_modifier(Modifier::ITALIC | Modifier::DIM),
        image_prefix: "📷 ",
        list_bullet: "• ",
        ordered_template: "{}. ",
        hr_char: '─',
        hr_style: Style::new().fg(theme.border),
    }
}

/// Header style: everything centered, date (italic) in sky colour.
fn header_style(theme: &crate::ui::theme::ColorTheme) -> MarkdownStyle {
    let mut s = base_md_style(theme);
    s.heading_1_alignment = Alignment::Center;
    s.paragraph_alignment = Alignment::Center;
    s.italic = ratatui::style::Style::new().fg(theme.sky);
    s
}

/// Body style: headings and paragraphs use the user's alignment.
fn body_style(theme: &crate::ui::theme::ColorTheme, body_alignment: Alignment) -> MarkdownStyle {
    let mut s = base_md_style(theme);
    s.heading_1_alignment = body_alignment;
    s.heading_2_alignment = body_alignment;
    s.heading_3_alignment = body_alignment;
    s.paragraph_alignment = body_alignment;
    s
}

/// Returns `true` when the description contains only link text (e.g. HN/Lobsters "Comments" links).
fn is_description_just_links(desc: &str) -> bool {
    let stripped = strip_html(desc).trim().to_string();
    let plain = strip_html_to_plain(desc).trim().to_string();
    !plain.is_empty() && stripped.is_empty()
}

/// Remove HTML tags from a string, also stripping `<a>` anchor content (comment links etc).
fn strip_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut skip = 0u32;
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '<' {
            if i + 3 < chars.len() && chars[i + 1] == '/' && chars[i + 2].eq_ignore_ascii_case(&'a')
            {
                skip = skip.saturating_sub(1);
            }
            if i + 2 < chars.len()
                && chars[i + 1].eq_ignore_ascii_case(&'a')
                && matches!(chars[i + 2], '>' | ' ' | '\n' | '\t')
            {
                skip += 1;
            }
            while i < chars.len() && chars[i] != '>' {
                i += 1;
            }
            if i < chars.len() {
                i += 1;
            }
            continue;
        }
        if skip == 0 {
            out.push(chars[i]);
        }
        i += 1;
    }
    out
}

/// Returns `true` when the description is full body content (not a summary).
fn is_body_like_description(article: &crate::models::Article) -> bool {
    let desc = &article.description;
    if desc.is_empty() {
        return true;
    }
    let plain = strip_html(desc);
    if (desc.contains("<p")
        || desc.contains("<div")
        || desc.contains("<br")
        || desc.contains("<table")
        || desc.contains("<blockquote"))
        && plain.len() > 500
    {
        return true;
    }
    plain.len() > 500
}

/// Strip HTML tags, keeping all text content, and normalize whitespace.
fn strip_html_to_plain(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    let mut result = String::with_capacity(out.len());
    let mut prev_ws = false;
    for ch in out.chars() {
        if ch.is_whitespace() {
            if !prev_ws {
                result.push(' ');
                prev_ws = true;
            }
        } else {
            result.push(ch);
            prev_ws = false;
        }
    }
    result.trim().to_string()
}

/// Helper to compute the rendered line count of lines at a given width.
fn count_lines(lines: &[ratatui::text::Line<'static>], width: u16) -> usize {
    Paragraph::new(lines.to_vec())
        .wrap(Wrap { trim: false })
        .line_count(width)
        .max(1)
}

/// Map a ratatui [`Color`] to the `tui_skeleton` colour enum for the skeleton widget.
fn to_skeleton_color(c: ratatui::style::Color) -> tui_skeleton::Color {
    match c {
        ratatui::style::Color::Rgb(r, g, b) => tui_skeleton::Color::Rgb(r, g, b),
        ratatui::style::Color::Indexed(i) => tui_skeleton::Color::Indexed(i),
        _ => tui_skeleton::Color::Reset,
    }
}

/// Returns `true` when a markdown line references `hero_url` as an image.
///
/// Catches all common markdown image formats:
/// - `![]({url})`
/// - `![alt]({url})`
/// - `[![]({url})](link)` (image wrapped in a link)
/// - `[![alt]({url})](link)`
fn is_hero_image_line(line: &str, hero_url: &str) -> bool {
    let trimmed = line.trim();
    let img_suffix = format!("]({})", hero_url);
    trimmed.contains("![") && trimmed.contains(&img_suffix)
}

/// Strip leading title and hero image from the body if they duplicate the header.
///
/// Many feeds include the article title and featured image at the start of their
/// `<content:encoded>` body, which would produce a duplicate right below the
/// header section. This function detects and removes that duplication.
fn dedup_body_header(body: &str, title: &str, hero_url: Option<&str>) -> String {
    let lines: Vec<&str> = body.lines().collect();
    let mut i = 0;

    while i < lines.len() && lines[i].trim().is_empty() {
        i += 1;
    }

    let h1_prefix = format!("# {}", title);
    if i < lines.len() && (lines[i].trim() == h1_prefix.as_str() || lines[i].trim() == title) {
        i += 1;
        if i < lines.len() && lines[i].trim().is_empty() {
            i += 1;
        }
    }

    let remaining: Vec<&str> = if let Some(url) = hero_url {
        lines[i..]
            .iter()
            .filter(|line| !is_hero_image_line(line, url))
            .copied()
            .collect()
    } else {
        lines[i..].to_vec()
    };

    remaining.join("\n").trim().to_string()
}

/// Build the header markdown (title, date, description, hero image, separator).
/// Rendered separately with centered alignment.
fn build_header_markdown(
    article: &crate::models::Article,
    header_image_url: Option<&str>,
) -> String {
    let mut parts: Vec<String> = Vec::new();

    parts.push(format!("# {}", article.title));
    parts.push(String::new());

    if let Some(secs) = article.published_secs {
        parts.push(format!("*{}*", format_pub_date(secs)));
        parts.push(String::new());
    }

    if !is_body_like_description(article) && !is_description_just_links(&article.description) {
        parts.push(strip_html_to_plain(&article.description));
        parts.push(String::new());
    }

    if let Some(url) = header_image_url {
        parts.push(format!("![]({})", url));
        parts.push(String::new());
    }

    parts.push("---".to_string());

    parts.join("\n")
}

/// Build the body markdown (deduped content + extra images).
/// Rendered separately with the user's body alignment.
fn build_body_markdown(
    article: &crate::models::Article,
    header_image_url: Option<&str>,
    extra_images: &[String],
) -> String {
    let mut parts: Vec<String> = Vec::new();

    let raw_body = if article.content.is_empty() && !article.description.is_empty() {
        strip_html(&article.description)
    } else {
        article.content.clone()
    };

    let body = dedup_body_header(&raw_body, &article.title, header_image_url);
    parts.push(body);

    for url in extra_images {
        if header_image_url.is_some_and(|h| url == h) {
            continue;
        }
        if !parts.iter().any(|p| p.contains(url)) {
            parts.push(String::new());
            parts.push(format!("![]({})", url));
        }
    }

    parts.join("\n")
}

// ── Render cache ──────────────────────────────────────────────────────────────

/// Rendered markdown of the article on screen, reused across frames while its key matches.
pub(crate) struct ArticleRender {
    key: u64,
    /// Header + body lines before inline images are injected.
    lines: Vec<ratatui::text::Line<'static>>,
    images: Vec<limner::ImageInfo>,
    /// Links in document order (used to map on-screen link text back to URLs).
    links: Vec<limner::LinkInfo>,
    header_line_count: usize,
    /// Width the lines were wrapped for (narrower when a scrollbar is reserved).
    render_width: u16,
    /// Lines with images injected, plus row positions; rebuilt when images arrive.
    layout: Option<ArticleLayout>,
}

/// Final on-screen layout of a cached render: images injected, preview hints
/// centred, and the first visual row of every line precomputed.
struct ArticleLayout {
    key: u64,
    width: u16,
    lines: Vec<ratatui::text::Line<'static>>,
    images: Vec<limner::ImageInfo>,
    placements: Vec<ImagePlacement>,
    /// `row_starts[i]` = first visual row of `lines[i]`; one extra entry = total rows.
    row_starts: Vec<usize>,
    /// Visual rows taken by the header (title, date, hero image…).
    header_rows: usize,
}

impl ArticleLayout {
    fn line_count(&self) -> usize {
        self.row_starts.last().copied().unwrap_or(0).max(1)
    }

    /// Positions (relative to the content area) of images intersecting the viewport.
    fn image_positions(
        &self,
        scroll: usize,
        width: u16,
        height: u16,
    ) -> Vec<(String, SignedPosition)> {
        use ratatui::layout::Alignment as LineAlign;
        self.placements
            .iter()
            .filter_map(|p| {
                let row = *self.row_starts.get(p.line_start)? as i64 - scroll as i64;
                let x = match p.alignment {
                    Some(LineAlign::Center) => (width as i64 - p.cell_cols as i64) / 2,
                    Some(LineAlign::Right) => width as i64 - p.cell_cols as i64,
                    _ => 0,
                };
                let visible = row + (p.cell_rows as i64) > 0 && row < height as i64;
                visible.then(|| {
                    let position = SignedPosition {
                        x: x as i16,
                        y: row.clamp(i16::MIN as i64, i16::MAX as i64) as i16,
                    };
                    (p.url.clone(), position)
                })
            })
            .collect()
    }
}

/// Rebuild the cached layout when the viewport height changes or images finish loading.
fn ensure_layout(app: &mut App, center_simple_body: bool, area: Rect) {
    use std::hash::{Hash, Hasher};
    let App {
        article_render,
        image_cache,
        protocol_cache,
        picker,
        ..
    } = app;
    let Some(render) = article_render.as_mut() else {
        return;
    };
    // Decoded images still waiting to become terminal protocols force a rebuild.
    let pending = render
        .images
        .iter()
        .any(|img| image_cache.contains_key(&img.url));
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (render.key, area.height, center_simple_body).hash(&mut h);
    for img in &render.images {
        protocol_cache
            .get(&img.url)
            .map(|p| (p.size().width, p.size().height))
            .hash(&mut h);
    }
    let key = h.finish();
    if !pending && render.layout.as_ref().is_some_and(|l| l.key == key) {
        return;
    }

    let width = render.render_width;
    let header_line_count = render.header_line_count;
    let mut lines = render.lines.clone();
    let mut images = render.images.clone();
    let mut placements = match picker {
        Some(picker) if !images.is_empty() => prepare_inline_images(
            &mut lines,
            &images,
            image_cache,
            protocol_cache,
            picker,
            &picker.font_size(),
            width,
            10,
        ),
        _ => Vec::new(),
    };

    // Header lines grow by the height of any injected header image.
    let effective_header_lines = header_line_count
        + images
            .iter()
            .take_while(|img| img.line_index < header_line_count)
            .filter_map(|img| protocol_cache.get(&img.url))
            .map(|p| (p.size().height as usize).saturating_sub(1))
            .sum::<usize>();

    let mut row_starts = row_starts(&lines, width);
    let total = row_starts.last().copied().unwrap_or(0);

    // Vertically centre preview hints below the header (skeleton handles itself).
    if center_simple_body && total < area.height as usize {
        let pad = (area.height as usize - total) / 2;
        let at = effective_header_lines.min(lines.len());
        lines.splice(
            at..at,
            std::iter::repeat_n(ratatui::text::Line::from(""), pad),
        );
        for img in &mut images {
            if img.line_index >= header_line_count {
                img.line_index += pad;
            }
        }
        for p in &mut placements {
            if p.line_start >= effective_header_lines {
                p.line_start += pad;
            }
        }
        row_starts = self::row_starts(&lines, width);
    }

    let header_rows = row_starts[effective_header_lines.min(lines.len())].max(1);
    render.layout = Some(ArticleLayout {
        key,
        width,
        lines,
        images,
        placements,
        row_starts,
        header_rows,
    });
}

/// First visual row of every line when wrapped at `width`, plus the total row count.
fn row_starts(lines: &[ratatui::text::Line<'static>], width: u16) -> Vec<usize> {
    let mut starts = Vec::with_capacity(lines.len() + 1);
    let mut row = 0;
    starts.push(0);
    for line in lines {
        row += Paragraph::new(line.clone())
            .wrap(Wrap { trim: false })
            .line_count(width);
        starts.push(row);
    }
    starts
}

#[cfg(test)]
impl ArticleRender {
    pub(crate) fn key(&self) -> u64 {
        self.key
    }
}

/// Hash of every input that affects the rendered markdown.
fn render_key(
    app: &App,
    article: &crate::models::Article,
    area: Rect,
    hide_scrollbar: bool,
    preview_hint_mode: bool,
    loading: bool,
) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (
        &article.link,
        &article.title,
        &article.description,
        &article.content,
        &article.images,
        article.published_secs,
        &article.error,
    )
        .hash(&mut h);
    (
        area.width,
        area.height,
        hide_scrollbar,
        preview_hint_mode,
        loading,
    )
        .hash(&mut h);
    format!("{:?}{:?}", app.body_alignment, app.theme).hash(&mut h);
    h.finish()
}

/// Render header and body markdown, re-wrapping narrower when a scrollbar will be shown.
fn render_markdown_parts(
    app: &App,
    header_md: &str,
    body_md: &str,
    simple_body: bool,
    content_area: Rect,
    hide_scrollbar: bool,
    key: u64,
) -> ArticleRender {
    let h_style = header_style(&app.theme);
    let mut b_style = body_style(&app.theme, app.body_alignment);
    if simple_body {
        b_style.paragraph = Style::new().fg(app.theme.muted);
        b_style.italic = Style::new().fg(app.theme.muted).italic();
        b_style.paragraph_alignment = Alignment::Center;
    }
    let render_at = |width: u16| {
        let hdr = render_markdown(header_md, &h_style, width);
        let bdy = render_markdown(body_md, &b_style, width);
        let header_line_count = hdr.lines.len();
        let mut lines = hdr.lines;
        lines.extend(bdy.lines);
        let mut images = hdr.images;
        images.extend(bdy.images.into_iter().map(|mut img| {
            img.line_index += header_line_count;
            img
        }));
        let mut links = hdr.links;
        links.extend(bdy.links);
        ArticleRender {
            key,
            lines,
            images,
            links,
            header_line_count,
            render_width: width,
            layout: None,
        }
    };
    let full = render_at(content_area.width);
    if !hide_scrollbar && count_lines(&full.lines, full.render_width) > content_area.height as usize
    {
        render_at(content_area.width.saturating_sub(2))
    } else {
        full
    }
}

/// Screen regions of the links drawn in `area`, each paired with its URL.
///
/// Found by scanning the rendered cells for the link style (link colour + underline),
/// so wrapping, alignment, scrolling, and injected images are all accounted for.
/// Each run of link cells is matched to a link by its text, in document order; a run
/// that continues the previous link's text (wrapped onto the next row) keeps its URL.
fn link_regions(
    buf: &ratatui::buffer::Buffer,
    area: Rect,
    link_color: ratatui::style::Color,
    links: &[limner::LinkInfo],
) -> Vec<(Rect, String)> {
    let is_link = |x: u16, y: u16| {
        let cell = &buf[(x, y)];
        cell.fg == link_color && cell.modifier.contains(Modifier::UNDERLINED)
    };
    let mut regions = Vec::new();
    // (link index, visible text matched so far) of the previous run.
    let mut last: Option<(usize, String)> = None;
    for y in area.top()..area.bottom() {
        let mut x = area.left();
        while x < area.right() {
            if !is_link(x, y) {
                x += 1;
                continue;
            }
            let start = x;
            let mut text = String::new();
            while x < area.right() && is_link(x, y) {
                text.push_str(buf[(x, y)].symbol());
                x += 1;
            }
            let needle = text.trim();
            if needle.is_empty() {
                continue;
            }
            // Word-wrapped links continue after a space, long URLs mid-word.
            let continued = last.as_ref().and_then(|(i, acc)| {
                [format!("{acc} {needle}"), format!("{acc}{needle}")]
                    .into_iter()
                    .find(|joined| links[*i].text.contains(joined.as_str()))
                    .map(|joined| (*i, joined))
            });
            let matched = continued.or_else(|| {
                let from = last.as_ref().map_or(0, |(i, _)| i + 1);
                (from..links.len())
                    .chain(0..from)
                    .find(|&i| links[i].text.contains(needle) || links[i].url.contains(needle))
                    .map(|i| (i, needle.to_string()))
            });
            if let Some((i, acc)) = matched {
                regions.push((Rect::new(start, y, x - start, 1), links[i].url.clone()));
                last = Some((i, acc));
            }
        }
    }
    regions
}

// ── Main entry point ──────────────────────────────────────────────────────────

/// Renders the article detail view with centered meta header, markdown body, scrolling,
/// and article metadata footer.
///
/// When `show_footer` is `false` (called from three-panel mode), the per-panel footer is
/// suppressed; `draw_three_panel` renders a single shared footer instead.
pub(super) fn draw_article_detail(
    f: &mut Frame,
    app: &mut App,
    area: Rect,
    is_preview: bool,
    show_footer: bool,
) {
    let article = get_selected_article(app);
    if article.is_none() && is_preview {
        let block = content_block("", false, app.user_data.border_rounded, &app.theme);
        let inner = block.inner(area);
        f.render_widget(block, area);
        f.render_widget(
            Paragraph::new("Select an article to preview.")
                .style(Style::default().fg(app.theme.muted)),
            inner,
        );
        return;
    }
    let Some(article) = article else { return };

    // Determine if the current feed is being refreshed (shown as skeleton overlay).
    let feed_refreshing = !app.in_saved_context
        && !app.in_category_context
        && app.feeds.get(app.selected_feed).is_some_and(|f| !f.fetched);

    let block = content_block("", !is_preview, app.user_data.border_rounded, &app.theme);
    let inner_area = block.inner(area);
    f.render_widget(block, area);

    let (content_area, bar_area) = if show_footer {
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(inner_area);
        (layout[0], layout[1])
    } else {
        (inner_area, Rect::default())
    };

    render_article_content(
        f,
        app,
        content_area,
        is_preview,
        feed_refreshing,
        false,
        &article,
    );

    if show_footer {
        draw_article_info(f, app, bar_area);
    }
}

/// Renders article content (markdown body, images, scrollbar) into the given area.
///
/// This is the shared rendering core used by both `draw_article_detail` and the
/// zen-mode full-screen view. The caller is responsible for borders and footers.
pub(crate) fn render_article_content(
    f: &mut Frame,
    app: &mut App,
    content_area: Rect,
    is_preview: bool,
    feed_refreshing: bool,
    hide_scrollbar: bool,
    article: &crate::models::Article,
) {
    // Recorded here (not by the caller) so zen mode is clickable/selectable too.
    app.hit.article_detail = content_area;
    // First image is the hero (MediaRSS) image, the rest are body extras.
    let header_image_url: Option<String> = article.images.first().cloned();
    let body_images: Vec<String> = match header_image_url {
        Some(_) => article.images[1..].to_vec(),
        None => article.images.clone(),
    };

    let preview_hint_mode =
        is_preview && article.content.is_empty() && article.error.is_none() && !feed_refreshing;
    let fetching_stub = app.article_fetching && article.content.len() < CONTENT_STUB_MAX_LEN;
    let loading = fetching_stub || feed_refreshing;

    // Build separate header and body markdown (only needed when the cache misses).
    let build_md = || {
        let header_md = build_header_markdown(article, header_image_url.as_deref());
        let body_md = if preview_hint_mode && !loading {
            String::from("*Full article not fetched. Press Enter to focus and read.*\n")
        } else if loading {
            String::new() // skeleton widget renders in place of body text
        } else if let Some(err) = &article.error {
            format!("*{err}*\n")
        } else {
            build_body_markdown(article, header_image_url.as_deref(), &body_images)
        };
        (header_md, body_md)
    };

    // Simple-body messages (hint/spinner) get centered styling and layout.
    let simple_body = preview_hint_mode || loading || article.error.is_some();

    // Markdown rendering is the expensive part of a frame: reuse it until any input changes.
    let key = render_key(
        app,
        article,
        content_area,
        hide_scrollbar,
        preview_hint_mode,
        loading,
    );
    if app.article_render.as_ref().is_none_or(|r| r.key != key) {
        let (header_md, body_md) = build_md();
        let render = render_markdown_parts(
            app,
            &header_md,
            &body_md,
            simple_body,
            content_area,
            hide_scrollbar,
            key,
        );
        app.article_render = Some(render);
    }
    ensure_layout(app, simple_body && !loading, content_area);
    let Some(layout) = app.article_render.as_ref().and_then(|r| r.layout.as_ref()) else {
        return;
    };
    let render_width = layout.width;
    let line_count = layout.line_count();
    // Clamp the saved offset to this layout: the same article wraps to fewer rows
    // in a wider view (e.g. zen), so an offset from a narrower view can overshoot.
    let max_scroll = line_count
        .saturating_sub(content_area.height as usize)
        .min(u16::MAX as usize) as u16;
    let scroll_offset = app.article_scroll.scroll_by(&article.link, 0, max_scroll);
    let scroll = scroll_offset as usize;

    // Only the lines intersecting the viewport are handed to ratatui, so frame cost
    // no longer grows with article length.
    let starts = &layout.row_starts;
    let first = starts.partition_point(|&r| r <= scroll).saturating_sub(1);
    let end = starts[..layout.lines.len()]
        .partition_point(|&r| r < scroll + content_area.height as usize)
        .max(first);
    let skip_rows = (scroll - starts[first].min(scroll)) as u16;
    let visible = layout.lines[first..end].to_vec();
    let image_positions = layout.image_positions(scroll, render_width, content_area.height);
    let header_visual = layout.header_rows;
    let images = layout.images.clone();

    // Only one article panel is drawn per frame, so the preview's metrics are the
    // ones wheel scrolling needs too.
    app.content_area_height = content_area.height;
    app.content_line_count = line_count;
    app.article_content_area = content_area;
    app.article_scroll_offset = scroll_offset;
    // Always store images so spawn_article_image_downloads picks them up in both
    // preview and full-detail mode.
    app.article_images = images;

    // Inline images — SlicedImage clips partially off-screen images itself.
    for (url, position) in image_positions {
        if let Some(sliced) = app.protocol_cache.get(&url) {
            f.render_widget(SlicedImage::new(sliced, position), content_area);
        }
    }

    let paragraph = Paragraph::new(visible)
        .wrap(Wrap { trim: false })
        .scroll((skip_rows, 0));
    let has_scrollbar = line_count > content_area.height as usize;
    let para_render_area = Rect {
        width: render_width,
        ..content_area
    };
    f.render_widget(paragraph, para_render_area);
    if let Some(cached) = &app.article_render {
        let regions = link_regions(
            f.buffer_mut(),
            para_render_area,
            app.theme.link,
            &cached.links,
        );
        app.hit.article_links = regions;
    }

    // ── Skeleton placeholder while article body is fetching ──
    if loading {
        let content_top = content_area.y as i32;
        let content_bottom = (content_area.y + content_area.height) as i32;
        let header_visible = header_visual.saturating_sub(scroll_offset as usize);
        let body_y = content_top + header_visible as i32;
        if body_y < content_bottom {
            let body_height = (content_bottom - body_y) as u16;
            let elapsed_ms = app.tick as u64 * 250;
            let block = SkeletonBlock::new(elapsed_ms)
                .mode(AnimationMode::Noise)
                .braille(true)
                .base(to_skeleton_color(app.theme.bg_dark))
                .highlight(to_skeleton_color(app.theme.sky));
            f.render_widget(
                block,
                Rect {
                    x: content_area.x,
                    y: body_y as u16,
                    width: content_area.width,
                    height: body_height,
                },
            );
        }
    }

    if has_scrollbar && !hide_scrollbar {
        let bar_area = Rect {
            x: content_area.x + content_area.width.saturating_sub(1),
            width: 1,
            ..content_area
        };
        render_scrollbar(
            f,
            bar_area,
            line_count,
            content_area.height as usize,
            scroll_offset as usize,
            &app.theme,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::buffer::Buffer;

    fn regions_for(md: &str, width: u16) -> Vec<(Rect, String)> {
        let theme = crate::ui::theme::ColorTheme::catppuccin_mocha();
        let rendered = render_markdown(md, &base_md_style(&theme), width);
        let area = Rect::new(0, 0, width, 20);
        let mut buf = Buffer::empty(area);
        use ratatui::widgets::Widget;
        Paragraph::new(rendered.lines)
            .wrap(Wrap { trim: false })
            .render(area, &mut buf);
        link_regions(&buf, area, theme.link, &rendered.links)
    }

    #[test]
    fn links_map_to_urls_including_duplicates_and_wraps() {
        let md = "See [here](https://a.test) and [here](https://b.test).\n\n\
                  Then [a much longer link text that wraps](https://c.test) end.";
        let regions = regions_for(md, 24);
        let urls: Vec<&str> = regions.iter().map(|(_, u)| u.as_str()).collect();
        assert!(urls.contains(&"https://a.test"), "{regions:?}");
        assert!(urls.contains(&"https://b.test"), "{regions:?}");
        // The long link spans several rows, all pointing at the same URL.
        let c_rows: Vec<u16> = regions
            .iter()
            .filter(|(_, u)| u == "https://c.test")
            .map(|(r, _)| r.y)
            .collect();
        assert!(c_rows.len() >= 2, "{regions:?}");
        // Duplicate texts resolve in document order.
        let a = regions
            .iter()
            .find(|(_, u)| u == "https://a.test")
            .unwrap()
            .0;
        let b = regions
            .iter()
            .find(|(_, u)| u == "https://b.test")
            .unwrap()
            .0;
        assert!((a.y, a.x) < (b.y, b.x));
    }

    #[test]
    fn plain_text_has_no_link_regions() {
        assert!(regions_for("Just some *styled* text.", 30).is_empty());
    }

    #[test]
    fn sliced_rendering_matches_full_paragraph() {
        use ratatui::widgets::Widget;
        let theme = crate::ui::theme::ColorTheme::catppuccin_mocha();
        let mut style = base_md_style(&theme);
        style.paragraph_alignment = Alignment::Center;
        let md: String = (0..40)
            .map(|i| format!("Paragraph {i} with enough words to wrap across rows.\n\n"))
            .collect();
        let lines = render_markdown(&md, &style, 30).lines;
        let width = 30;
        let starts = row_starts(&lines, width);
        assert_eq!(
            *starts.last().unwrap(),
            Paragraph::new(lines.clone())
                .wrap(Wrap { trim: false })
                .line_count(width)
        );
        let area = Rect::new(0, 0, width, 10);
        for scroll in [0usize, 1, 7, 33, 90] {
            let mut full = Buffer::empty(area);
            Paragraph::new(lines.clone())
                .wrap(Wrap { trim: false })
                .scroll((scroll as u16, 0))
                .render(area, &mut full);
            let first = starts.partition_point(|&r| r <= scroll).saturating_sub(1);
            let end = starts[..lines.len()]
                .partition_point(|&r| r < scroll + area.height as usize)
                .max(first);
            let mut sliced = Buffer::empty(area);
            Paragraph::new(lines[first..end].to_vec())
                .wrap(Wrap { trim: false })
                .scroll(((scroll - starts[first]) as u16, 0))
                .render(area, &mut sliced);
            assert_eq!(full, sliced, "scroll {scroll}");
        }
    }
}
