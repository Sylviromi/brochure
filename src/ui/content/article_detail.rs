//! Article detail view rendering with markdown content, scrolling, and header.
//!
//! The header (title, date, description, hero image) and body are combined into a
//! single markdown document so that everything scrolls together.

use crate::{app::App, handlers::article::get_selected_article, models::CONTENT_STUB_MAX_LEN};
use limner::{
    Alignment, MarkdownStyle,
    render_image::{
        Image, ImageViewport, compute_image_render_rects, fit_cell_size, make_clipped_protocol,
        prepare_inline_images,
    },
    render_markdown,
};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect, Size},
    style::{Modifier, Style},
    widgets::{Paragraph, Wrap},
};

use super::super::{content_block, render_scrollbar};
use super::{footer::draw_article_footer, utils::format_pub_date};
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
        code_block_alignment: Alignment::Left,
        link: Style::new().fg(theme.link).underlined(),
        link_prefix: "🔗 ",
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
        draw_article_footer(f, app, bar_area, true);
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

    // Build separate header and body markdown.
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

    // Simple-body messages (hint/spinner) get centered styling and layout.
    let simple_body = preview_hint_mode || loading || article.error.is_some();

    // Render with independent alignment styles.
    let h_style = header_style(&app.theme);
    let mut b_style = body_style(&app.theme, app.body_alignment);
    let content_width = content_area.width;

    if simple_body {
        b_style.paragraph = Style::new().fg(app.theme.muted);
        b_style.italic = Style::new().fg(app.theme.muted).italic();
        b_style.paragraph_alignment = Alignment::Center;
    }

    let mut render_width = content_width;

    // Helper: render header + body at a given width and merge the results.
    let render_and_merge = |w| -> (
        Vec<ratatui::text::Line<'static>>,
        Vec<limner::ImageInfo>,
        Vec<limner::LinkInfo>,
        usize,
    ) {
        let hdr = render_markdown(&header_md, &h_style, w);
        let bdy = render_markdown(&body_md, &b_style, w);
        let header_line_count = hdr.lines.len();
        let mut lines = hdr.lines;
        lines.extend(bdy.lines);
        let mut images = hdr.images;
        for mut img in bdy.images {
            img.line_index += header_line_count;
            images.push(img);
        }
        let mut links = hdr.links;
        let link_offset = header_line_count;
        for mut lnk in bdy.links {
            lnk.line_index += link_offset;
            links.push(lnk);
        }
        (lines, images, links, header_line_count)
    };

    let (mut result_lines, mut result_images, mut result_links, mut header_line_count) =
        render_and_merge(render_width);

    if !hide_scrollbar && count_lines(&result_lines, render_width) > content_area.height as usize {
        render_width = render_width.saturating_sub(2);
        let (lines, images, links, hlc) = render_and_merge(render_width);
        result_lines = lines;
        result_images = images;
        result_links = links;
        header_line_count = hlc;
    }

    // Scroll offset for the entire unified content.
    let scroll_offset = app.article_scroll.get(&article.link);

    // Inject inline images into the rendered line buffer.
    let font_size = app.picker.as_ref().map(|p| p.font_size());
    let mut placements = if let Some(picker) = &app.picker
        && !app.image_cache.is_empty()
    {
        let font_size = font_size.as_ref().expect("picker exists");
        prepare_inline_images(
            &mut result_lines,
            &result_images,
            &app.image_cache,
            &mut app.protocol_cache,
            picker,
            font_size,
            render_width,
            10,
        )
    } else {
        Vec::new()
    };

    // Recompute header line count accounting for injected header images.
    let effective_header_lines = {
        let mut h = header_line_count;
        if let Some(font_size) = &font_size {
            for img in &result_images {
                if img.line_index >= header_line_count {
                    break;
                }
                if !app.protocol_cache.contains_key(&img.url) {
                    continue;
                }
                let Some(dyn_img) = app.image_cache.get(&img.url) else {
                    continue;
                };
                let (_cols, rows) = fit_cell_size(dyn_img, font_size, render_width, 10);
                h += rows as usize - 1;
            }
        }
        h
    };

    // Recompute line count after image injection.
    let mut line_count = count_lines(&result_lines, render_width);

    // Vertically center preview hints below the header (skeleton handles itself).
    if simple_body && !loading && line_count < content_area.height as usize {
        let pad = (content_area.height as usize - line_count) / 2;
        let blank = ratatui::text::Line::from("");
        let body_part = result_lines.split_off(effective_header_lines);
        result_lines.extend(std::iter::repeat_n(blank, pad));
        result_lines.extend(body_part);
        for img in &mut result_images {
            if img.line_index >= header_line_count {
                img.line_index += pad;
            }
        }
        for lnk in &mut result_links {
            if lnk.line_index >= header_line_count {
                lnk.line_index += pad;
            }
        }
        for p in &mut placements {
            if p.line_start >= effective_header_lines {
                p.line_start += pad;
            }
        }
        line_count += pad;
    }

    if !is_preview {
        app.content_area_height = content_area.height;
        app.content_line_count = line_count;
    }

    // Always store images so spawn_article_image_downloads picks them up in both
    // preview and full-detail mode.
    app.article_images = result_images;
    app.article_links = result_links;

    // Store render metadata in the full-detail view only.
    if !is_preview {
        app.article_content_area = content_area;
        app.article_scroll_offset = scroll_offset;
    }

    // Render inline images with cut-off clipping — partially off-screen images
    // now show their visible portion instead of being skipped entirely.
    if !placements.is_empty() {
        let viewport = ImageViewport {
            content: Rect {
                width: render_width,
                ..content_area
            },
            scroll: scroll_offset,
        };
        let render_rects = compute_image_render_rects(&placements, &result_lines, &viewport);
        for rr in &render_rects {
            let protocol = if rr.hidden_top > 0 || rr.hidden_left > 0 {
                let Some(img) = app.image_cache.get(&rr.url) else {
                    continue;
                };
                let full_size = Size::new(rr.full_cols, rr.full_rows);
                let visible_size = Size::new(rr.render_rect.width, rr.render_rect.height);
                let Some(picker) = &app.picker else { continue };
                let Some(proto) = make_clipped_protocol(
                    picker,
                    img,
                    full_size,
                    visible_size,
                    rr.hidden_top,
                    rr.hidden_left,
                ) else {
                    continue;
                };
                proto
            } else if let Some(protocol) = app.protocol_cache.get(&rr.url) {
                protocol.clone()
            } else {
                continue;
            };
            f.render_widget(Image::new(&protocol), rr.render_rect);
        }
    }

    // ── Render the unified paragraph ──
    // Cache the visual header height before result_lines moves into the paragraph.
    let header_visual = {
        let end = effective_header_lines.min(result_lines.len());
        Paragraph::new(result_lines[..end].to_vec())
            .wrap(Wrap { trim: false })
            .line_count(render_width)
            .max(1) as usize
    };
    let paragraph = Paragraph::new(result_lines)
        .wrap(Wrap { trim: false })
        .scroll((scroll_offset, 0));

    let has_scrollbar = line_count > content_area.height as usize;
    let para_render_area = Rect {
        width: render_width,
        ..content_area
    };
    f.render_widget(paragraph, para_render_area);

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
