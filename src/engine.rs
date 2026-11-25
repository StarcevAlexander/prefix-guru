use std::collections::HashMap;
use std::sync::OnceLock;

use postcss_guru::{Message, NodeId, NodeKind, Tree};

use crate::at_rule::AtRule;
use crate::browsers::Browsers;
use crate::data::{AGENTS, FEATURES};
use crate::decl_hacks::DECL_HACKS;
use crate::declaration::{Decl, DeclHack};
use crate::selector_hacks::SELECTOR_HACKS;
use crate::selectors::Selector;
use crate::supports::Supports;
use crate::utils::{remove_note, uniq};
use crate::value_hacks::VALUE_HACKS;
use crate::values::Value;
use crate::vendor;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flexbox {
    All,
    No2009,
    None,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    pub browsers: Option<Vec<String>>,
    pub add: bool,
    pub remove: bool,
    pub cascade: bool,
    pub flexbox: Flexbox,
    pub supports: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            browsers: None,
            add: true,
            remove: true,
            cascade: true,
            flexbox: Flexbox::All,
            supports: true,
        }
    }
}

impl Options {
    pub(crate) fn grid_disabled(&self) -> bool {
        true
    }
}

#[derive(Clone, Default)]
pub(crate) struct NodeCache {
    pub prefix: Option<Option<String>>,
    pub values: Option<Vec<(String, String)>>,
    pub prefixeds: Vec<(String, Vec<(String, String)>)>,
    pub max: Option<usize>,
    pub cascade: Option<bool>,
    pub disabled: Option<bool>,
    pub self_disabled: bool,
}

pub(crate) struct Group {
    pub rule: Option<NodeId>,
    pub index: usize,
    pub length: usize,
    pub unprefixed: String,
}

pub(crate) struct State<'a> {
    pub prefixes: &'a Prefixes,
    pub cache: HashMap<NodeId, NodeCache>,
    pub messages: Vec<Message>,
}

impl<'a> State<'a> {
    pub fn new(prefixes: &'a Prefixes) -> Self {
        Self {
            prefixes,
            cache: HashMap::new(),
            messages: Vec::new(),
        }
    }

    pub fn cache(&mut self, id: NodeId) -> &mut NodeCache {
        self.cache.entry(id).or_default()
    }

    pub fn cached(&self, id: NodeId) -> NodeCache {
        self.cache.get(&id).cloned().unwrap_or_default()
    }
}

pub(crate) struct Session<'s, 'a> {
    pub tree: &'s mut Tree,
    pub state: &'s mut State<'a>,
}

impl<'s, 'a> Session<'s, 'a> {
    pub fn new(tree: &'s mut Tree, state: &'s mut State<'a>) -> Self {
        Self { tree, state }
    }

    pub fn warn(&mut self, node: NodeId, text: impl Into<String>) {
        self.state.messages.push(
            Message::warning(text)
                .at(self.tree, node)
                .by("autoprefixer"),
        );
    }

    pub fn clone(&mut self, id: NodeId) -> NodeId {
        let copy = self.tree.clone_node(id);
        if let Some(mut cache) = self.state.cache.get(&id).cloned() {
            cache.prefix = None;
            cache.values = None;
            self.state.cache.insert(copy, cache);
        }
        copy
    }

    pub fn clone_full(&mut self, id: NodeId) -> NodeId {
        let copy = self.tree.clone_node(id);
        if let Some(cache) = self.state.cache.get(&id).cloned() {
            self.state.cache.insert(copy, cache);
        }
        copy
    }

    pub fn parent_prefix(&mut self, id: NodeId) -> Option<String> {
        if let Some(cached) = self.cached(id).prefix {
            return cached;
        }
        static PATTERN: OnceLock<regex::Regex> = OnceLock::new();
        let pattern = PATTERN.get_or_init(|| regex::Regex::new(r":(-[0-9A-Za-z_]+-)").unwrap());
        let mut chain = vec![id];
        let mut node = id;
        let found = loop {
            let kind = self.tree.kind(node);
            if kind == NodeKind::Decl && self.tree.prop(node).starts_with('-') {
                break Some(vendor::prefix(self.tree.prop(node)));
            }
            if kind == NodeKind::Root {
                break None;
            }
            if kind == NodeKind::Rule
                && self.tree.selector(node).contains(":-")
                && let Some(found) = pattern
                    .captures(self.tree.selector(node))
                    .map(|captures| captures[1].to_string())
            {
                break Some(found);
            }
            if kind == NodeKind::AtRule && self.tree.name(node).starts_with('-') {
                break Some(vendor::prefix(self.tree.name(node)));
            }
            match self.tree.parent(node) {
                Some(parent) => {
                    node = parent;
                    let cached = self.cached(node).prefix;
                    chain.push(node);
                    if let Some(cached) = cached {
                        break cached;
                    }
                }
                None => break None,
            }
        };
        let found = found.filter(|found| Browsers::prefixes().contains(found));
        for id in chain {
            self.cache(id).prefix = Some(found.clone());
        }
        found
    }

    pub fn group(&self, decl: NodeId) -> Group {
        let rule = self.tree.parent(decl);
        Group {
            rule,
            index: self.tree.index_of(decl).unwrap_or(0),
            length: rule.map_or(0, |rule| self.tree.child_count(rule)),
            unprefixed: self.prefixes.unprefixed(self.tree.prop(decl)),
        }
    }

    pub fn group_check(
        &mut self,
        group: &Group,
        step: isize,
        mut callback: impl FnMut(&mut Self, NodeId) -> bool,
    ) -> bool {
        let Some(rule) = group.rule else {
            return false;
        };
        let nodes: Vec<NodeId> = self.tree.children(rule).to_vec();
        let mut index = group.index as isize + step;
        while index >= 0 && (index as usize) < group.length {
            let other = nodes[index as usize];
            if self.tree.is(other, NodeKind::Decl) {
                let prop = self.tree.prop(other).to_string();
                if step == -1
                    && prop == group.unprefixed
                    && !Browsers::with_prefix(self.tree.value(other))
                {
                    break;
                }
                if self.prefixes.unprefixed(&prop) != group.unprefixed {
                    break;
                } else if callback(self, other) {
                    return true;
                }
                if step == 1
                    && prop == group.unprefixed
                    && !Browsers::with_prefix(self.tree.value(other))
                {
                    break;
                }
            }
            index += step;
        }
        false
    }

    pub fn parent_has(&self, node: NodeId, mut test: impl FnMut(&Tree, NodeId) -> bool) -> bool {
        self.tree.parent(node).is_some_and(|parent| {
            self.tree
                .children(parent)
                .iter()
                .any(|child| test(self.tree, *child))
        })
    }
}

impl<'s, 'a> std::ops::Deref for Session<'s, 'a> {
    type Target = State<'a>;

    fn deref(&self) -> &State<'a> {
        self.state
    }
}

impl<'s, 'a> std::ops::DerefMut for Session<'s, 'a> {
    fn deref_mut(&mut self) -> &mut State<'a> {
        self.state
    }
}

#[derive(Clone, Default)]
pub(crate) struct Entry {
    pub decl: Option<Decl>,
    pub values: Vec<Value>,
}

#[derive(Clone, Default)]
pub(crate) struct RemoveEntry {
    pub remove: bool,
    pub values: Vec<crate::values::OldValue>,
}

pub struct Prefixes {
    pub options: Options,
    pub(crate) browsers: Browsers,
    pub(crate) decls: HashMap<String, Entry>,
    pub(crate) selectors: Vec<Selector>,
    pub(crate) at_rules: HashMap<String, AtRule>,
    pub(crate) resolution: Option<AtRule>,
    pub(crate) supports: Option<Supports>,
    pub(crate) remove_decls: HashMap<String, RemoveEntry>,
    pub(crate) remove_selectors: Vec<crate::selectors::OldSelector>,
    pub(crate) remove_resolution: Option<AtRule>,
    cleaner: OnceLock<Box<Prefixes>>,
}

impl std::fmt::Debug for Prefixes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Prefixes")
            .field("browsers", &self.browsers.selected)
            .finish()
    }
}

type Selection = Vec<(String, Vec<String>)>;

impl Prefixes {
    pub fn new(options: Options) -> Result<Self, crate::browsers::QueryError> {
        let browsers = Browsers::new(options.browsers.as_deref())?;
        Ok(Self::with_browsers(options, browsers))
    }

    pub(crate) fn with_browsers(options: Options, browsers: Browsers) -> Self {
        let mut prefixes = Self {
            options,
            browsers,
            decls: HashMap::new(),
            selectors: Vec::new(),
            at_rules: HashMap::new(),
            resolution: None,
            supports: Some(Supports::new()),
            remove_decls: HashMap::new(),
            remove_selectors: Vec::new(),
            remove_resolution: None,
            cleaner: OnceLock::new(),
        };
        let (add, remove) = prefixes.select();
        prefixes.preprocess(&add, &remove);
        prefixes
    }

    fn select(&self) -> (Selection, Selection) {
        let mut add = Vec::new();
        let mut remove = Vec::new();
        for feature in FEATURES {
            let entries = feature_browsers(feature);
            let notes = uniq(
                &entries
                    .iter()
                    .filter(|(_, note)| !note.is_empty())
                    .map(|(browser, note)| format!("{} {note}", Browsers::prefix(browser)))
                    .collect::<Vec<_>>(),
            );
            let mut selected: Vec<String> = entries
                .iter()
                .filter(|(browser, _)| self.browsers.is_selected(browser))
                .map(|(browser, note)| {
                    let prefix = Browsers::prefix(browser);
                    if note.is_empty() {
                        prefix
                    } else {
                        format!("{prefix} {note}")
                    }
                })
                .collect();
            selected = sort(&uniq(&selected));
            if self.options.flexbox == Flexbox::No2009 {
                selected.retain(|prefix| !prefix.contains("2009"));
            }
            let mut all: Vec<String> = entries
                .iter()
                .map(|(browser, _)| Browsers::prefix(browser))
                .collect();
            all.extend(feature.mistakes.iter().map(|mistake| mistake.to_string()));
            all.extend(notes);
            let all = uniq(&all);
            if selected.is_empty() {
                remove.push((feature.name.to_string(), all));
                continue;
            }
            let obsolete: Vec<String> = all
                .iter()
                .filter(|prefix| !selected.contains(prefix))
                .cloned()
                .collect();
            add.push((feature.name.to_string(), selected.clone()));
            if selected.len() < all.len() {
                remove.push((feature.name.to_string(), obsolete));
            }
        }
        (add, remove)
    }

    fn preprocess(&mut self, add: &Selection, remove: &Selection) {
        for (name, prefixes) in add {
            let feature = feature(name);
            if name == "@keyframes" || name == "@viewport" {
                self.at_rules.insert(
                    name.clone(),
                    AtRule {
                        prefixes: prefixes.clone(),
                    },
                );
            } else if name == "@resolution" {
                self.resolution = Some(AtRule {
                    prefixes: prefixes.clone(),
                });
            } else if feature.selector {
                self.selectors.push(Selector::load(name, prefixes.clone()));
            } else if !feature.props.is_empty() {
                let value = Value::load(name, prefixes.clone());
                for prop in feature.props {
                    self.decls
                        .entry(prop.to_string())
                        .or_default()
                        .values
                        .push(value.clone());
                }
            } else {
                let values = self
                    .decls
                    .remove(name)
                    .map_or_else(Vec::new, |entry| entry.values);
                self.decls.insert(
                    name.clone(),
                    Entry {
                        decl: Some(Decl::load(name, prefixes.clone())),
                        values,
                    },
                );
            }
        }
        for (name, prefixes) in remove {
            let feature = feature(name);
            if feature.selector {
                let selector = Selector::load(name, prefixes.clone());
                for prefix in prefixes {
                    self.remove_selectors.push(selector.old(prefix));
                }
            } else if name == "@keyframes" || name == "@viewport" {
                for prefix in prefixes {
                    let prefixed = format!("@{prefix}{}", &name[1..]);
                    self.remove_decls.insert(
                        prefixed,
                        RemoveEntry {
                            remove: true,
                            values: Vec::new(),
                        },
                    );
                }
            } else if name == "@resolution" {
                self.remove_resolution = Some(AtRule {
                    prefixes: prefixes.clone(),
                });
            } else if !feature.props.is_empty() {
                let value = Value::load(name, Vec::new());
                for prefix in prefixes {
                    if let Some(old) = value.old(prefix) {
                        for prop in feature.props {
                            self.remove_decls
                                .entry(prop.to_string())
                                .or_default()
                                .values
                                .push(old.clone());
                        }
                    }
                }
            } else {
                for prefix in prefixes {
                    let mut skip = false;
                    if name == "align-self"
                        && let Some(decl) = self.add_decl(name)
                    {
                        skip = prefix == "-webkit- 2009"
                            && decl.prefixes.iter().any(|p| p == "-webkit-")
                            || prefix == "-webkit-"
                                && decl.prefixes.iter().any(|p| p == "-webkit- 2009");
                    }
                    if skip {
                        continue;
                    }
                    for prefixed in self.decl(name).old(name, prefix) {
                        self.remove_decls.entry(prefixed).or_default().remove = true;
                    }
                }
            }
        }
    }

    pub(crate) fn cleaner(&self) -> &Prefixes {
        self.cleaner.get_or_init(|| {
            Box::new(Self::with_browsers(
                self.options.clone(),
                Browsers::with_selected(Vec::new()),
            ))
        })
    }

    pub fn browsers(&self) -> &[String] {
        &self.browsers.selected
    }

    pub(crate) fn decl(&self, name: &str) -> Decl {
        Decl::load(name, Vec::new())
    }

    pub fn normalize(&self, name: &str) -> String {
        let decl = self.decl(name);
        decl.hack.normalize(name)
    }

    pub fn prefixed(&self, prop: &str, prefix: &str) -> String {
        let prop = vendor::unprefixed(prop);
        let decl = self.decl(&prop);
        decl.hack.prefixed(&prop, prefix)
    }

    pub(crate) fn unprefixed(&self, prop: &str) -> String {
        let mut value = self.normalize(&vendor::unprefixed(prop));
        if value == "flex-direction" {
            value = String::from("flex-flow");
        }
        value
    }

    pub(crate) fn add_decl(&self, prop: &str) -> Option<&Decl> {
        self.decls.get(prop).and_then(|entry| entry.decl.as_ref())
    }

    pub(crate) fn add_values(&self, prop: &str) -> Vec<Value> {
        let global = self.decls.get("*").map(|entry| entry.values.clone());
        let values = self.decls.get(prop).map(|entry| entry.values.clone());
        match (global, values) {
            (Some(global), Some(values)) => {
                let mut all = global;
                all.extend(values);
                all
            }
            (Some(global), None) => global,
            (None, Some(values)) => values,
            (None, None) => Vec::new(),
        }
    }

    pub(crate) fn remove_decl(&self, prop: &str) -> Option<&RemoveEntry> {
        self.remove_decls.get(prop)
    }

    pub(crate) fn remove_values(&self, prop: &str) -> Vec<crate::values::OldValue> {
        let global = self.remove_decls.get("*").map(|entry| entry.values.clone());
        let values = self
            .remove_decls
            .get(prop)
            .map(|entry| entry.values.clone());
        match (global, values) {
            (Some(global), Some(values)) => {
                let mut all = global;
                all.extend(values);
                all
            }
            (Some(global), None) => global,
            (None, Some(values)) => values,
            (None, None) => Vec::new(),
        }
    }
}

fn feature_browsers(feature: &crate::data::Feature) -> Vec<(String, &'static str)> {
    feature
        .runs
        .iter()
        .flat_map(|(agent, from, to, note)| {
            let versions = AGENTS
                .iter()
                .find(|known| known.name == *agent)
                .expect("agent is known")
                .versions;
            (*from..=*to).map(move |at| (format!("{agent} {}", versions[at as usize]), *note))
        })
        .collect()
}

fn sort(prefixes: &[String]) -> Vec<String> {
    let mut sorted = prefixes.to_vec();
    sorted.sort_by(|a, b| {
        let a_length = remove_note(a).len();
        let b_length = remove_note(b).len();
        b_length.cmp(&a_length).then_with(|| b.len().cmp(&a.len()))
    });
    sorted
}

pub(crate) fn feature(name: &str) -> &'static crate::data::Feature {
    FEATURES
        .iter()
        .find(|feature| feature.name == name)
        .expect("feature is known")
}

pub(crate) fn decl_hack(name: &str) -> &'static dyn DeclHack {
    DECL_HACKS
        .iter()
        .rev()
        .find(|(names, _)| names.contains(&name))
        .map_or(&crate::declaration::BASE, |(_, hack)| *hack)
}

pub(crate) fn value_hack(name: &str) -> &'static dyn crate::values::ValueHack {
    VALUE_HACKS
        .iter()
        .rev()
        .find(|(names, _)| names.contains(&name))
        .map_or(&crate::values::BASE, |(_, hack)| *hack)
}

pub(crate) fn selector_hack(name: &str) -> &'static dyn crate::selectors::SelectorHack {
    SELECTOR_HACKS
        .iter()
        .rev()
        .find(|(names, _)| names.contains(&name))
        .map_or(&crate::selectors::BASE, |(_, hack)| *hack)
}
