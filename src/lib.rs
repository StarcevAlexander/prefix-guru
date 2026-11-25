mod at_rule;
mod browsers;
mod data;
mod decl_hacks;
mod declaration;
mod engine;
mod processor;
mod selector_hacks;
mod selectors;
mod supports;
mod transition;
mod utils;
mod value;
mod value_hacks;
mod values;
mod vendor;

use postcss_guru::{CssSyntaxError, Message, Output, Plugin, Tree};

pub use browsers::QueryError;
pub use engine::{Flexbox, Options, Prefixes};

const TIME_CAPSULE: &str = "Autoprefixer target browsers do not need any prefixes.You do not need Autoprefixer anymore.\nCheck your Browserslist config to be sure that your targets are set up correctly.\n\n  Learn more at:\n  https://github.com/postcss/autoprefixer#readme\n  https://github.com/browserslist/browserslist#readme\n\n";

pub fn browserslist(queries: &[&str]) -> Result<Vec<String>, QueryError> {
    let queries: Vec<String> = queries.iter().map(|query| query.to_string()).collect();
    browsers::resolve_queries(&queries)
}

pub fn defaults() -> Vec<String> {
    let queries: Vec<String> = browsers::DEFAULTS
        .iter()
        .map(|query| query.to_string())
        .collect();
    browsers::resolve_queries(&queries).unwrap_or_default()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GridError(pub String);

impl std::fmt::Display for GridError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug)]
pub enum AutoprefixError {
    Query(QueryError),
    Syntax(CssSyntaxError),
    Grid(GridError),
}

impl std::fmt::Display for AutoprefixError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AutoprefixError::Query(error) => write!(f, "{error}"),
            AutoprefixError::Syntax(error) => write!(f, "{error}"),
            AutoprefixError::Grid(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for AutoprefixError {}

impl From<QueryError> for AutoprefixError {
    fn from(error: QueryError) -> Self {
        AutoprefixError::Query(error)
    }
}

pub struct Autoprefixer {
    prefixes: Prefixes,
}

impl Autoprefixer {
    pub fn new(options: Options) -> Result<Self, QueryError> {
        Ok(Self {
            prefixes: Prefixes::new(options)?,
        })
    }

    pub fn browsers(&self) -> &[String] {
        self.prefixes.browsers()
    }

    pub fn process(&self, css: &str) -> Result<Output, AutoprefixError> {
        let mut tree = postcss_guru::parse(css).map_err(AutoprefixError::Syntax)?;
        processor::check_grid_support(&tree)
            .map_err(GridError)
            .map_err(AutoprefixError::Grid)?;
        let mut messages = Vec::new();
        self.run(&mut tree, &mut messages);
        let css = tree.to_css();
        Ok(Output {
            css,
            messages,
            tree,
        })
    }

    fn time_capsule(&self, messages: &mut Vec<Message>) {
        if self.prefixes.browsers().is_empty()
            || !self.prefixes.selectors.is_empty()
            || !self.prefixes.decls.is_empty()
            || !self.prefixes.at_rules.is_empty()
            || self.prefixes.resolution.is_some()
        {
            return;
        }
        messages.push(Message::warning(TIME_CAPSULE).by("autoprefixer"));
    }
}

impl Plugin for Autoprefixer {
    fn name(&self) -> &str {
        "prefix-guru"
    }

    fn run(&self, tree: &mut Tree, messages: &mut Vec<Message>) {
        if let Err(mode) = processor::check_grid_support(tree) {
            messages.push(
                Message::warning(format!(
                    "prefix-guru does not implement the IE Grid translation, so it was not applied: {mode}"
                ))
                .by("prefix-guru"),
            );
            return;
        }
        self.time_capsule(messages);
        let mut state = engine::State::new(&self.prefixes);
        if self.prefixes.options.remove {
            processor::Processor::remove(tree, &mut state);
        }
        if self.prefixes.options.add {
            processor::Processor::add(tree, &mut state);
        }
        messages.append(&mut state.messages);
    }
}

pub fn process(css: &str, options: Options) -> Result<Output, AutoprefixError> {
    Autoprefixer::new(options)?.process(css)
}
