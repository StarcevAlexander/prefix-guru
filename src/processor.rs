use std::sync::OnceLock;

use postcss_guru::{NodeId, NodeKind, Tree};
use regex::Regex;

use crate::engine::{Flexbox, Session, State};
use crate::transition::Transition;
use crate::value;
use crate::values;

pub(crate) struct Processor;

impl Processor {
    pub(crate) fn remove(tree: &mut Tree, state: &mut State) {
        let prefixes = state.prefixes;
        tree.walk_at_rules_mut(None, |tree, rule| {
            let mut ctx = Session::new(tree, state);
            let name = ctx.tree.name(rule).to_string();
            if prefixes.remove_decl(&format!("@{name}")).is_some() {
                if !disabled(&mut ctx, rule) {
                    ctx.tree.remove(rule);
                }
            } else if name == "media"
                && ctx.tree.params(rule).contains("-resolution")
                && let Some(resolution) = &prefixes.remove_resolution
            {
                resolution.clean_resolution(&mut ctx, rule);
            }
            true
        });
        tree.walk_rules_mut(|tree, rule| {
            let mut ctx = Session::new(tree, state);
            if disabled(&mut ctx, rule) {
                return true;
            }
            let matched = prefixes
                .remove_selectors
                .iter()
                .any(|checker| checker.check(&ctx, rule));
            if matched {
                ctx.tree.remove(rule);
            }
            true
        });
        tree.walk_decls_mut(None, |tree, decl| {
            let mut ctx = Session::new(tree, state);
            if disabled(&mut ctx, decl) {
                return true;
            }
            let prop = ctx.tree.prop(decl).to_string();
            let mut unprefixed = ctx.prefixes.unprefixed(&prop);
            if prop == "transition" || prop == "transition-property" {
                Transition.remove(&mut ctx, decl);
            }
            if prefixes
                .remove_decl(&prop)
                .is_some_and(|entry| entry.remove)
            {
                let mut not_hack = ctx.group_check(&ctx.group(decl), 1, |ctx, other| {
                    ctx.prefixes.normalize(ctx.tree.prop(other)) == unprefixed
                });
                if unprefixed == "flex-flow" {
                    not_hack = true;
                }
                if prop == "-webkit-box-orient"
                    && !ctx.parent_has(decl, |tree, child| {
                        matches!(tree.prop(child), "flex-direction" | "flex-flow")
                    })
                {
                    return true;
                }
                if not_hack && !with_hack_value(&ctx, decl) {
                    if ctx.tree.raw_before(decl).contains('\n') {
                        reduce_spaces(&mut ctx, decl);
                    }
                    ctx.tree.remove(decl);
                    return true;
                }
            }
            for checker in prefixes.remove_values(&unprefixed) {
                if !checker.check(ctx.tree.value(decl)) {
                    continue;
                }
                unprefixed = checker.unprefixed.clone();
                let group = ctx.group(decl);
                let not_hack = ctx.group_check(&group, 1, |ctx, other| {
                    ctx.tree.value(other).contains(unprefixed.as_str())
                });
                if not_hack {
                    ctx.tree.remove(decl);
                    return true;
                }
            }
            true
        });
    }

    pub(crate) fn add(tree: &mut Tree, state: &mut State) {
        let prefixes = state.prefixes;
        tree.walk_at_rules_mut(None, |tree, rule| {
            let mut ctx = Session::new(tree, state);
            let name = ctx.tree.name(rule).to_string();
            if name == "keyframes" {
                if !disabled(&mut ctx, rule)
                    && let Some(keyframes) = prefixes.at_rules.get("@keyframes")
                {
                    keyframes.process(&mut ctx, rule);
                }
            } else if name == "viewport" {
                if !disabled(&mut ctx, rule)
                    && let Some(viewport) = prefixes.at_rules.get("@viewport")
                {
                    viewport.process(&mut ctx, rule);
                }
            } else if name == "supports" {
                if prefixes.options.supports
                    && !disabled(&mut ctx, rule)
                    && let Some(supports) = &prefixes.supports
                {
                    supports.process(&mut ctx, rule);
                }
            } else if name == "media"
                && ctx.tree.params(rule).contains("-resolution")
                && !disabled(&mut ctx, rule)
                && let Some(resolution) = &prefixes.resolution
            {
                resolution.process_resolution(&mut ctx, rule);
            }
            true
        });
        tree.walk_rules_mut(|tree, rule| {
            let mut ctx = Session::new(tree, state);
            if disabled(&mut ctx, rule) {
                return true;
            }
            for selector in &prefixes.selectors {
                selector.process(&mut ctx, rule);
            }
            true
        });
        tree.walk_decls_mut(None, |tree, decl| {
            let mut ctx = Session::new(tree, state);
            if disabled_decl(&mut ctx, decl) {
                return true;
            }
            add_declaration(&mut ctx, decl);
            true
        });
        tree.walk_decls_mut(None, |tree, decl| {
            let mut ctx = Session::new(tree, state);
            if disabled_value(&mut ctx, decl) {
                return true;
            }
            let unprefixed = ctx.prefixes.unprefixed(ctx.tree.prop(decl));
            for value in ctx.prefixes.add_values(&unprefixed) {
                value.process(&mut ctx, decl);
            }
            values::save(ctx.prefixes, &mut ctx, decl);
            true
        });
    }
}

fn add_declaration(ctx: &mut Session, decl: NodeId) {
    let prop = ctx.tree.prop(decl).to_string();
    let value = ctx.tree.value(decl).to_string();
    if prop == "color-adjust" {
        if !ctx.parent_has(decl, |tree, child| tree.prop(child) == "print-color-adjust") {
            ctx.warn(
                decl,
                "Replace color-adjust to print-color-adjust. The color-adjust shorthand is currently deprecated.",
            );
        }
    } else if prop == "grid-row-span" {
        ctx.warn(
            decl,
            "grid-row-span is not part of final Grid Layout. Use grid-row.",
        );
        return;
    } else if prop == "grid-column-span" {
        ctx.warn(
            decl,
            "grid-column-span is not part of final Grid Layout. Use grid-column.",
        );
        return;
    } else if prop == "display" && value == "box" {
        ctx.warn(
            decl,
            "You should write display: flex by final spec instead of display: box",
        );
        return;
    } else if prop == "text-emphasis-position" {
        if value == "under" || value == "over" {
            ctx.warn(
                decl,
                "You should use 2 values for text-emphasis-position For example, `under left` instead of just `under`.",
            );
        }
    } else if prop == "text-decoration-skip" && value == "ink" {
        ctx.warn(
            decl,
            "Replace text-decoration-skip: ink to text-decoration-skip-ink: auto, because spec had been changed",
        );
    } else {
        if value.contains("radial-gradient") {
            static OLD_RADIAL: OnceLock<Regex> = OnceLock::new();
            if OLD_RADIAL
                .get_or_init(|| {
                    Regex::new(r"(?i)(^|[^-])radial-gradient\(\s*\d+(\w*|%)\s+\d+(\w*|%)\s*,")
                        .unwrap()
                })
                .is_match(&value)
            {
                ctx.warn(
                    decl,
                    "Gradient has outdated direction syntax. New syntax is like `closest-side at 0 0` instead of `0 0, closest-side`.",
                );
            } else {
                for node in value::parse(&value) {
                    if node.is_function("radial-gradient")
                        && let Some(nodes) = node.nodes()
                    {
                        for word in nodes {
                            if word.is_word("cover") {
                                ctx.warn(
                                    decl,
                                    "Gradient has outdated direction syntax. Replace `cover` to `farthest-corner`.",
                                );
                            } else if word.is_word("contain") {
                                ctx.warn(
                                    decl,
                                    "Gradient has outdated direction syntax. Replace `contain` to `closest-side`.",
                                );
                            }
                        }
                    }
                }
            }
        }
        if value.contains("linear-gradient") {
            static OLD_LINEAR: OnceLock<Regex> = OnceLock::new();
            if OLD_LINEAR
                .get_or_init(|| {
                    Regex::new(r"(?i)(^|[^-])linear-gradient\(\s*(top|left|right|bottom)").unwrap()
                })
                .is_match(&value)
            {
                ctx.warn(
                    decl,
                    "Gradient has outdated direction syntax. New syntax is like `to left` instead of `right`.",
                );
            }
        }
    }
    const SIZES: &[&str] = &[
        "width",
        "height",
        "min-width",
        "max-width",
        "min-height",
        "max-height",
        "inline-size",
        "min-inline-size",
        "max-inline-size",
        "block-size",
        "min-block-size",
        "max-block-size",
    ];
    if SIZES.contains(&prop.as_str()) && !value.contains("-fill-available") {
        if value.contains("fill-available") {
            ctx.warn(
                decl,
                "Replace fill-available to stretch, because spec had been changed",
            );
        } else if value.contains("fill") {
            let has_fill = value::parse(&value).iter().any(|node| node.is_word("fill"));
            if has_fill {
                ctx.warn(
                    decl,
                    "Replace fill to stretch, because spec had been changed",
                );
            }
        }
    }
    let prefixes = ctx.prefixes;
    if prop == "transition" || prop == "transition-property" {
        Transition.add(ctx, decl);
    } else if prop == "align-self" {
        let display = display_type(ctx, decl);
        if display.as_deref() != Some("grid")
            && prefixes.options.flexbox != Flexbox::None
            && let Some(prefixer) = prefixes.add_decl("align-self")
        {
            prefixer.hack.process(prefixer, ctx, decl);
        }
    } else if prop != "justify-self"
        && prop != "place-self"
        && let Some(prefixer) = prefixes.add_decl(&prop)
    {
        prefixer.hack.process(prefixer, ctx, decl);
    }
}

fn display_type(ctx: &Session, decl: NodeId) -> Option<String> {
    let parent = ctx.tree.parent(decl)?;
    for child in ctx.tree.children(parent) {
        if ctx.tree.prop(*child) != "display" {
            continue;
        }
        let value = ctx.tree.value(*child);
        if value.contains("flex") {
            return Some(String::from("flex"));
        }
        if value.contains("grid") {
            return Some(String::from("grid"));
        }
    }
    None
}

pub(crate) fn disabled(ctx: &mut Session, node: NodeId) -> bool {
    if let Some(cached) = ctx.cached(node).disabled {
        return cached;
    }
    static IGNORE_NEXT: OnceLock<Regex> = OnceLock::new();
    static ON_OFF: OnceLock<Regex> = OnceLock::new();
    static ON: OnceLock<Regex> = OnceLock::new();
    let ignore_next = IGNORE_NEXT
        .get_or_init(|| Regex::new(r"(?i)(!\s*)?autoprefixer:\s*ignore\s+next").unwrap());
    let on_off = ON_OFF.get_or_init(|| Regex::new(r"(?i)(!\s*)?autoprefixer:\s*(off|on)").unwrap());
    let on = ON.get_or_init(|| Regex::new(r"(?i)on").unwrap());
    if let Some(previous) = ctx.tree.prev_sibling(node)
        && ctx.tree.is(previous, NodeKind::Comment)
        && ignore_next.is_match(ctx.tree.comment(previous))
    {
        ctx.cache(node).disabled = Some(true);
        ctx.cache(node).self_disabled = true;
        return true;
    }
    let mut value: Option<bool> = None;
    let container =
        matches!(ctx.tree.kind(node), NodeKind::Root | NodeKind::Rule) || ctx.tree.has_block(node);
    if container {
        let mut status: Option<bool> = None;
        for child in ctx.tree.children(node).to_vec() {
            if ctx.tree.is(child, NodeKind::Comment) && on_off.is_match(ctx.tree.comment(child)) {
                if status.is_some() {
                    ctx.warn(child, "Second Autoprefixer control comment was ignored. Autoprefixer applies control comment to whole block, not to next rules.");
                } else {
                    status = Some(on.is_match(ctx.tree.comment(child)));
                }
            }
        }
        if let Some(status) = status {
            value = Some(!status);
        }
    }
    if !container || value.is_none() {
        if let Some(parent) = ctx.tree.parent(node) {
            if ctx.cached(parent).self_disabled {
                value = Some(false);
            } else {
                value = Some(disabled(ctx, parent));
            }
        } else {
            value = Some(false);
        }
    }
    let value = value.unwrap_or(false);
    ctx.cache(node).disabled = Some(value);
    value
}

fn disabled_decl(ctx: &mut Session, node: NodeId) -> bool {
    let prop = ctx.tree.prop(node).to_string();
    if prop.contains("grid") || prop == "justify-items" {
        return true;
    }
    if ctx.prefixes.options.flexbox == Flexbox::None
        && (prop.contains("flex")
            || ["order", "justify-content", "align-items", "align-content"]
                .contains(&prop.as_str()))
    {
        return true;
    }
    disabled(ctx, node)
}

fn disabled_value(ctx: &mut Session, node: NodeId) -> bool {
    let prop = ctx.tree.prop(node).to_string();
    let value = ctx.tree.value(node).to_string();
    if prop == "display" && value.contains("grid") {
        return true;
    }
    if ctx.prefixes.options.flexbox == Flexbox::None && prop == "display" && value.contains("flex")
    {
        return true;
    }
    if prop == "content" {
        return true;
    }
    disabled(ctx, node)
}

fn reduce_spaces(ctx: &mut Session, decl: NodeId) {
    if ctx.group_check(&ctx.group(decl), -1, |_, _| true) {
        return;
    }
    let before = ctx.tree.raw_before(decl);
    let parts: Vec<&str> = before.split('\n').collect();
    let prev_min = parts.last().map_or(0, |part| part.len());
    let mut diff: Option<usize> = None;
    ctx.group_check(&ctx.group(decl), 1, |ctx, other| {
        let before = ctx.tree.raw_before(other);
        let mut parts: Vec<String> = before.split('\n').map(String::from).collect();
        let last = parts.len() - 1;
        if parts[last].len() > prev_min {
            let difference = *diff.get_or_insert(parts[last].len() - prev_min);
            let keep = parts[last].len().saturating_sub(difference);
            parts[last].truncate(keep);
            ctx.tree.raws_mut(other).before = Some(parts.join("\n"));
        }
        false
    });
}

fn with_hack_value(ctx: &Session, decl: NodeId) -> bool {
    ctx.tree.prop(decl) == "-webkit-background-clip" && ctx.tree.value(decl) == "text"
        || ctx.tree.prop(decl) == "-webkit-box-orient"
            && ctx.parent_has(decl, |tree, child| tree.prop(child) == "-webkit-line-clamp")
}

pub(crate) fn check_grid_support(tree: &Tree) -> Result<(), String> {
    static GRID: OnceLock<Regex> = OnceLock::new();
    let grid = GRID.get_or_init(|| {
        Regex::new(r"(?i)(!\s*)?autoprefixer\s*grid:\s*(on|off|(no-)?autoplace)").unwrap()
    });
    let mut found: Option<String> = None;
    tree.walk_comments(|tree, comment| {
        if found.is_none()
            && let Some(captures) = grid.captures(tree.comment(comment))
            && captures.get(2).is_some_and(|mode| mode.as_str() != "off")
        {
            found = Some(captures[2].to_string());
        }
        true
    });
    if let Some(mode) = found {
        return Err(format!(
            "the `autoprefixer: grid: {mode}` control comment enables the IE Grid translation, which prefix-guru does not implement"
        ));
    }
    if let Ok(env) = std::env::var("AUTOPREFIXER_GRID")
        && !env.is_empty()
        && env != "off"
    {
        return Err(format!(
            "AUTOPREFIXER_GRID={env} enables the IE Grid translation, which prefix-guru does not implement"
        ));
    }
    Ok(())
}
