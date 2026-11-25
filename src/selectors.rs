use std::sync::OnceLock;

use postcss_guru::{NodeId, NodeKind};
use regex::Regex;

use crate::browsers::Browsers;
use crate::engine::{self, Session};
use crate::utils;

#[derive(Clone)]
pub(crate) struct Selector {
    pub name: String,
    pub prefixes: Vec<String>,
    pub hack: &'static dyn SelectorHack,
}

impl Selector {
    pub(crate) fn load(name: &str, prefixes: Vec<String>) -> Self {
        let hack = engine::selector_hack(name);
        Self {
            name: name.to_string(),
            prefixes: hack.prefixes(prefixes),
            hack,
        }
    }

    pub(crate) fn prefixed(&self, prefix: &str) -> String {
        self.hack.prefixed(&self.name, prefix)
    }

    pub(crate) fn possible(&self) -> Vec<String> {
        self.hack.possible()
    }

    pub(crate) fn regexp(&self, prefix: Option<&str>) -> Regex {
        let name = prefix.map_or_else(|| self.name.clone(), |prefix| self.prefixed(prefix));
        Regex::new(&format!(
            r#"(?i)(^|[^:"'=]){}"#,
            utils::escape_regexp(&name)
        ))
        .unwrap()
    }

    pub(crate) fn replace(&self, selector: &str, prefix: &str) -> String {
        self.regexp(None)
            .replace_all(selector, format!("$1{}", self.prefixed(prefix)))
            .into_owned()
    }

    pub(crate) fn check(&self, ctx: &Session, rule: NodeId) -> bool {
        let selector = ctx.tree.selector(rule);
        selector.contains(self.name.as_str()) && self.regexp(None).is_match(selector)
    }

    pub(crate) fn prefixeds(&self, ctx: &mut Session, rule: NodeId) -> Vec<(String, String)> {
        if let Some((_, found)) = ctx
            .cached(rule)
            .prefixeds
            .iter()
            .find(|(name, _)| name == &self.name)
        {
            return found.clone();
        }
        let selector = ctx.tree.selector(rule).to_string();
        let mut prefixeds = Vec::new();
        if selector.contains(',') {
            let to_process: Vec<String> = utils::comma(&selector)
                .into_iter()
                .filter(|part| part.contains(self.name.as_str()))
                .collect();
            for prefix in self.possible() {
                let joined = to_process
                    .iter()
                    .map(|part| self.replace(part, &prefix))
                    .collect::<Vec<_>>()
                    .join(", ");
                prefixeds.push((prefix, joined));
            }
        } else {
            for prefix in self.possible() {
                prefixeds.push((prefix.clone(), self.replace(&selector, &prefix)));
            }
        }
        ctx.cache(rule)
            .prefixeds
            .push((self.name.clone(), prefixeds.clone()));
        prefixeds
    }

    fn already(
        &self,
        ctx: &Session,
        rule: NodeId,
        prefixeds: &[(String, String)],
        prefix: &str,
    ) -> bool {
        let Some(parent) = ctx.tree.parent(rule) else {
            return false;
        };
        let nodes: Vec<NodeId> = ctx.tree.children(parent).to_vec();
        let mut index = ctx.tree.index_of(rule).unwrap_or(0) as isize - 1;
        while index >= 0 {
            let before = nodes[index as usize];
            if ctx.tree.kind(before) != NodeKind::Rule {
                return false;
            }
            let selector = ctx.tree.selector(before);
            let mut some = false;
            for (key, prefixed) in prefixeds {
                if selector == prefixed {
                    if key == prefix {
                        return true;
                    }
                    some = true;
                    break;
                }
            }
            if !some {
                return false;
            }
            index -= 1;
        }
        false
    }

    pub(crate) fn process(&self, ctx: &mut Session, rule: NodeId) {
        if !self.check(ctx, rule) {
            return;
        }
        let parent = ctx.parent_prefix(rule);
        for prefix in &self.prefixes {
            if parent
                .as_ref()
                .is_none_or(|parent| parent == utils::remove_note(prefix))
            {
                self.add(ctx, rule, prefix);
            }
        }
    }

    pub(crate) fn add(&self, ctx: &mut Session, rule: NodeId, prefix: &str) {
        let prefixeds = self.prefixeds(ctx, rule);
        if self.already(ctx, rule, &prefixeds, prefix) {
            return;
        }
        let selector = prefixeds
            .iter()
            .find(|(key, _)| key == prefix)
            .map_or_else(String::new, |(_, selector)| selector.clone());
        let cloned = ctx.clone(rule);
        ctx.tree.set_selector(cloned, selector);
        ctx.tree.insert_before(rule, cloned);
    }

    pub(crate) fn old(&self, prefix: &str) -> OldSelector {
        let prefixed = self.prefixed(prefix);
        OldSelector {
            prefixed: prefixed.clone(),
            regexp: self.regexp(Some(prefix)),
            prefixeds: self
                .possible()
                .into_iter()
                .map(|prefix| (self.prefixed(&prefix), self.regexp(Some(&prefix))))
                .collect(),
            unprefixed: self.name.clone(),
            name_regexp: self.regexp(None),
        }
    }
}

#[derive(Clone)]
pub(crate) struct OldSelector {
    pub prefixed: String,
    pub unprefixed: String,
    regexp: Regex,
    name_regexp: Regex,
    prefixeds: Vec<(String, Regex)>,
}

impl OldSelector {
    pub(crate) fn check(&self, ctx: &Session, rule: NodeId) -> bool {
        let selector = ctx.tree.selector(rule);
        selector.contains(self.prefixed.as_str())
            && self.regexp.is_match(selector)
            && !self.is_hack(ctx, rule)
    }

    fn is_hack(&self, ctx: &Session, rule: NodeId) -> bool {
        let Some(parent) = ctx.tree.parent(rule) else {
            return true;
        };
        let nodes: Vec<NodeId> = ctx.tree.children(parent).to_vec();
        let mut index = ctx.tree.index_of(rule).unwrap_or(0) + 1;
        while index < nodes.len() {
            let before = nodes[index];
            if ctx.tree.kind(before) != NodeKind::Rule {
                return true;
            }
            let before = ctx.tree.selector(before);
            if before.contains(self.unprefixed.as_str()) && self.name_regexp.is_match(before) {
                return false;
            }
            let some = self.prefixeds.iter().any(|(string, regexp)| {
                before.contains(string.as_str()) && regexp.is_match(before)
            });
            if !some {
                return true;
            }
            index += 1;
        }
        true
    }
}

pub(crate) trait SelectorHack: Sync {
    fn prefixes(&self, prefixes: Vec<String>) -> Vec<String> {
        prefixes
    }

    fn prefixed(&self, name: &str, prefix: &str) -> String {
        static LEADING: OnceLock<Regex> = OnceLock::new();
        LEADING
            .get_or_init(|| Regex::new(r"^(\W*)").unwrap())
            .replace(name, format!("$1{prefix}"))
            .into_owned()
    }

    fn possible(&self) -> Vec<String> {
        Browsers::prefixes().to_vec()
    }
}

pub(crate) struct Base;

impl SelectorHack for Base {}

pub(crate) static BASE: Base = Base;
