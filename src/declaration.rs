use std::sync::OnceLock;

use postcss_guru::{NodeId, NodeKind};

use crate::browsers::Browsers;
use crate::engine::{Session, decl_hack};
use crate::utils::{remove_note, uniq};

#[derive(Clone)]
pub(crate) struct Decl {
    pub prefixes: Vec<String>,
    pub hack: &'static dyn DeclHack,
}

impl Decl {
    pub(crate) fn load(name: &str, prefixes: Vec<String>) -> Self {
        let hack = decl_hack(name);
        Self {
            prefixes: hack.prefixes(prefixes),
            hack,
        }
    }

    pub(crate) fn old(&self, prop: &str, prefix: &str) -> Vec<String> {
        self.hack.old(prop, prefix)
    }
}

pub(crate) trait DeclHack: Sync {
    fn prefixes(&self, prefixes: Vec<String>) -> Vec<String> {
        prefixes
    }

    fn prefixed(&self, prop: &str, prefix: &str) -> String {
        format!("{prefix}{prop}")
    }

    fn normalize(&self, prop: &str) -> String {
        prop.to_string()
    }

    fn old(&self, prop: &str, prefix: &str) -> Vec<String> {
        vec![self.prefixed(prop, prefix)]
    }

    fn check(&self, _ctx: &Session, _node: NodeId) -> bool {
        true
    }

    fn add(
        &self,
        decl: &Decl,
        ctx: &mut Session,
        node: NodeId,
        prefix: &str,
        prefixes: &[String],
    ) -> Option<NodeId> {
        base_add(self, decl, ctx, node, prefix, prefixes)
    }

    fn insert(
        &self,
        decl: &Decl,
        ctx: &mut Session,
        node: NodeId,
        prefix: &str,
        prefixes: &[String],
    ) -> Option<NodeId> {
        base_insert(self, decl, ctx, node, prefix, prefixes)
    }

    fn set(&self, ctx: &mut Session, node: NodeId, prefix: &str) -> Option<NodeId> {
        let prefixed = self.prefixed(ctx.tree.prop(node), prefix);
        ctx.tree.set_prop(node, prefixed);
        Some(node)
    }

    fn process(&self, decl: &Decl, ctx: &mut Session, node: NodeId) -> Vec<String> {
        base_process(self, decl, ctx, node)
    }
}

pub(crate) struct Base;

impl DeclHack for Base {}

pub(crate) static BASE: Base = Base;

pub(crate) fn base_add<T: DeclHack + ?Sized>(
    hack: &T,
    decl: &Decl,
    ctx: &mut Session,
    node: NodeId,
    prefix: &str,
    prefixes: &[String],
) -> Option<NodeId> {
    let prefixed = hack.prefixed(ctx.tree.prop(node), prefix);
    if is_already(ctx, node, &prefixed) || other_prefixes(ctx, node, prefix) {
        return None;
    }
    hack.insert(decl, ctx, node, prefix, prefixes)
}

pub(crate) fn is_already(ctx: &mut Session, node: NodeId, prefixed: &str) -> bool {
    let group = ctx.group(node);
    let has = |ctx: &mut Session, other: NodeId| ctx.tree.prop(other) == prefixed;
    ctx.group_check(&group, -1, has) || ctx.group_check(&group, 1, has)
}

fn other_prefixes(ctx: &Session, node: NodeId, prefix: &str) -> bool {
    static VAR: OnceLock<regex::Regex> = OnceLock::new();
    let var = VAR.get_or_init(|| regex::Regex::new(r"var\([^)]+\)").unwrap());
    let value = ctx.tree.value(node);
    for other in Browsers::prefixes() {
        if other == prefix {
            continue;
        }
        if value.contains(other.as_str()) {
            return var.replace(value, "").contains(other.as_str());
        }
    }
    false
}

pub(crate) fn base_insert<T: DeclHack + ?Sized>(
    hack: &T,
    decl: &Decl,
    ctx: &mut Session,
    node: NodeId,
    prefix: &str,
    prefixes: &[String],
) -> Option<NodeId> {
    let clone = ctx.clone(node);
    let cloned = hack.set(ctx, clone, prefix)?;
    let prop = ctx.tree.prop(cloned).to_string();
    let value = ctx.tree.value(cloned).to_string();
    if ctx.parent_has(node, |tree, child| {
        tree.is(child, NodeKind::Decl) && tree.prop(child) == prop && tree.value(child) == value
    }) {
        return None;
    }
    if need_cascade(decl, ctx, node) {
        let before = calc_before(decl, ctx, node, prefixes, prefix);
        ctx.tree.raws_mut(cloned).before = Some(before);
    }
    Some(ctx.tree.insert_before(node, cloned))
}

pub(crate) fn calc_before(
    decl: &Decl,
    ctx: &mut Session,
    node: NodeId,
    prefixes: &[String],
    prefix: &str,
) -> String {
    let max = max_prefixed(decl, ctx, node, prefixes);
    let diff = max.saturating_sub(remove_note(prefix).len());
    let mut before = ctx.tree.raw_before(node);
    if diff > 0 {
        before.push_str(&" ".repeat(diff));
    }
    before
}

pub(crate) fn max_prefixed(
    _decl: &Decl,
    ctx: &mut Session,
    node: NodeId,
    prefixes: &[String],
) -> usize {
    if let Some(max) = ctx.cached(node).max {
        return max;
    }
    let max = prefixes
        .iter()
        .map(|prefix| remove_note(prefix).len())
        .max()
        .unwrap_or(0);
    ctx.cache(node).max = Some(max);
    max
}

pub(crate) fn need_cascade(decl: &Decl, ctx: &mut Session, node: NodeId) -> bool {
    let cached = ctx.cached(node).cascade;
    if let Some(cached) = cached {
        return cached;
    }
    let cascade = ctx.prefixes.options.cascade && ctx.tree.raw_before(node).contains('\n');
    ctx.cache(node).cascade = Some(cascade);
    let _ = decl;
    cascade
}

pub(crate) fn base_process<T: DeclHack + ?Sized>(
    hack: &T,
    decl: &Decl,
    ctx: &mut Session,
    node: NodeId,
) -> Vec<String> {
    if !need_cascade(decl, ctx, node) {
        return engine_process(hack, decl, ctx, node);
    }
    let prefixes = engine_process(hack, decl, ctx, node);
    if prefixes.is_empty() {
        return prefixes;
    }
    restore_before(decl, ctx, node);
    let before = calc_before(decl, ctx, node, &prefixes, "");
    ctx.tree.raws_mut(node).before = Some(before);
    prefixes
}

pub(crate) fn engine_process<T: DeclHack + ?Sized>(
    hack: &T,
    decl: &Decl,
    ctx: &mut Session,
    node: NodeId,
) -> Vec<String> {
    if !hack.check(ctx, node) {
        return Vec::new();
    }
    let parent = ctx.parent_prefix(node);
    let prefixes: Vec<String> = decl
        .prefixes
        .iter()
        .filter(|prefix| {
            parent
                .as_ref()
                .is_none_or(|parent| parent == remove_note(prefix))
        })
        .cloned()
        .collect();
    let mut added: Vec<String> = Vec::new();
    for prefix in prefixes {
        let mut passed = added.clone();
        passed.push(prefix.clone());
        if hack.add(decl, ctx, node, &prefix, &passed).is_some() {
            added.push(prefix);
        }
    }
    added
}

fn restore_before(decl: &Decl, ctx: &mut Session, node: NodeId) {
    let mut lines: Vec<String> = ctx
        .tree
        .raw_before(node)
        .split('\n')
        .map(String::from)
        .collect();
    let mut min = lines.last().cloned().unwrap_or_default();
    let group = ctx.group(node);
    ctx.group_check(&group, -1, |ctx, other| {
        let last = ctx
            .tree
            .raw_before(other)
            .rsplit('\n')
            .next()
            .unwrap_or("")
            .to_string();
        if last.len() < min.len() {
            min = last;
        }
        false
    });
    if let Some(last) = lines.last_mut() {
        *last = min;
    }
    ctx.tree.raws_mut(node).before = Some(lines.join("\n"));
    let _ = decl;
}

pub(crate) fn flex_spec(prefix: &str) -> (Spec, String) {
    let spec = match prefix {
        "-webkit- 2009" | "-moz-" => Spec::V2009,
        "-ms-" => Spec::V2012,
        "-webkit-" => Spec::Final,
        _ => Spec::Unknown,
    };
    let prefix = if prefix == "-webkit- 2009" {
        String::from("-webkit-")
    } else {
        prefix.to_string()
    };
    (spec, prefix)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Spec {
    V2009,
    V2012,
    Final,
    Unknown,
}

pub(crate) fn replace_first(text: &str, pattern: &str, replacement: &str) -> String {
    text.replacen(pattern, replacement, 1)
}

pub(crate) fn map_prefixes(prefixes: Vec<String>, from: &str, to: &str) -> Vec<String> {
    uniq(
        &prefixes
            .into_iter()
            .map(|prefix| {
                if prefix == from {
                    to.to_string()
                } else {
                    prefix
                }
            })
            .collect::<Vec<_>>(),
    )
}
