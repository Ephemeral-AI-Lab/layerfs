//! Last-writer-wins rules from `/.lww`.
//!
//! The file holds gitignore-style patterns, one per line; `#` starts a
//! comment, `!` negates, a trailing `/` is ignored, a pattern containing `/`
//! is anchored at the root, `*` and `?` stay within one name and `**` spans
//! names. `%include <file>` reads another file of the same tree (for example
//! `.gitignore`) in place. The last matching rule decides; a negated one
//! clears the match. A path both sides changed that a rule matches takes the
//! side with the newer modification time, ours on a tie, instead of merging.

use std::collections::BTreeSet;

use crate::error::ContentResult;

/// Path of the rules file.
pub const RULES_FILE: &str = "/.lww";
/// Rules files one load may read, the included ones counted.
pub const MAXIMUM_RULE_FILES: usize = 16;

#[derive(Clone, Debug, Eq, PartialEq)]
enum Token {
    Char(char),
    /// `?`: one character other than `/`.
    One,
    /// `*`: any run without `/`.
    Name,
    /// `**`: any run.
    Any,
    /// `**/` at a name boundary: zero or more complete directory names.
    Directories,
}

/// One parsed rule.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Rule {
    /// The rule line as written.
    pub pattern: String,
    negated: bool,
    anchored: bool,
    tokens: Vec<Token>,
}

/// An ordered rule list.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Rules {
    rules: Vec<Rule>,
}

impl Rules {
    /// Loads `/.lww` and its includes through `read`, which returns a file's
    /// text or `None` when the path is absent or not a text file.
    pub fn load(
        read: &mut dyn FnMut(&str) -> ContentResult<Option<String>>,
    ) -> ContentResult<Self> {
        let mut rules = Self::default();
        let mut seen = BTreeSet::new();
        rules.read_file(RULES_FILE, read, &mut seen)?;
        Ok(rules)
    }

    /// One group of patterns in the rules-file syntax, in order: the last
    /// matching pattern decides and a negated one clears the match (#352
    /// F-057 merge units).
    pub fn of(patterns: &[String]) -> Self {
        Self {
            rules: patterns.iter().map(|pattern| parse(pattern)).collect(),
        }
    }

    /// True when no rule exists.
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    fn read_file(
        &mut self,
        path: &str,
        read: &mut dyn FnMut(&str) -> ContentResult<Option<String>>,
        seen: &mut BTreeSet<String>,
    ) -> ContentResult<()> {
        if seen.len() >= MAXIMUM_RULE_FILES || !seen.insert(path.to_owned()) {
            return Ok(());
        }
        let Some(text) = read(path)? else {
            return Ok(());
        };
        for line in text.split('\n') {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(rest) = line.strip_prefix("%include") {
                let include = rest.trim();
                if rest.starts_with(char::is_whitespace)
                    && !include.is_empty()
                    && !include.contains(char::is_whitespace)
                {
                    let path = format!("/{}", include.trim_start_matches('/'));
                    self.read_file(&path, read, seen)?;
                    continue;
                }
            }
            self.rules.push(parse(line));
        }
        Ok(())
    }

    /// The deciding rule for an absolute path, if any.
    pub fn matching(&self, path: &str) -> Option<&Rule> {
        let path: Vec<char> = path.trim_start_matches('/').chars().collect();
        let mut hit = None;
        for rule in &self.rules {
            if rule.matches(&path, false) {
                hit = (!rule.negated).then_some(rule);
            }
        }
        hit
    }
}

fn parse(line: &str) -> Rule {
    let negated = line.starts_with('!');
    let body = if negated { &line[1..] } else { line };
    let body = body.strip_suffix('/').unwrap_or(body);
    let anchored = body.contains('/');
    let body = body.strip_prefix('/').unwrap_or(body);
    let mut tokens = Vec::new();
    let mut chars = body.chars().peekable();
    while let Some(c) = chars.next() {
        tokens.push(match c {
            '*' if chars.peek() == Some(&'*') => {
                chars.next();
                if matches!(
                    tokens.last(),
                    None | Some(Token::Char('/') | Token::Directories)
                ) && chars.peek() == Some(&'/')
                {
                    chars.next();
                    Token::Directories
                } else {
                    Token::Any
                }
            }
            '*' => Token::Name,
            '?' => Token::One,
            other => Token::Char(other),
        });
    }
    Rule {
        pattern: line.to_owned(),
        negated,
        anchored,
        tokens,
    }
}

impl Rule {
    /// One pattern in the rules-file syntax, used as a path glob.
    pub fn glob(pattern: &str) -> Self {
        parse(pattern)
    }

    /// True when the pattern matches the whole absolute path, not only one
    /// of its parent directories.
    pub fn matches_path(&self, path: &str) -> bool {
        let path: Vec<char> = path.trim_start_matches('/').chars().collect();
        self.matches(&path, true)
    }

    /// Matches a whole run of names: starting at the path start (anchored) or
    /// after any `/`, ending at the path end or, unless `whole`, before a `/`.
    fn matches(&self, path: &[char], whole: bool) -> bool {
        let starts = std::iter::once(0).chain(if self.anchored {
            Vec::new()
        } else {
            (0..path.len())
                .filter(|&i| path[i] == '/')
                .map(|i| i + 1)
                .collect()
        });
        starts
            .into_iter()
            .any(|start| self.matches_from(path, start, whole))
    }

    fn matches_from(&self, path: &[char], start: usize, whole: bool) -> bool {
        let n = path.len();
        let mut current = vec![false; n + 1];
        current[start] = true;
        for token in &self.tokens {
            let mut next = vec![false; n + 1];
            match token {
                Token::Char(c) => {
                    for p in 0..n {
                        next[p + 1] = current[p] && path[p] == *c;
                    }
                }
                Token::One => {
                    for p in 0..n {
                        next[p + 1] = current[p] && path[p] != '/';
                    }
                }
                Token::Name => {
                    for p in 0..=n {
                        if current[p] {
                            next[p] = true;
                            let mut q = p;
                            while q < n && path[q] != '/' {
                                q += 1;
                                next[q] = true;
                            }
                        }
                    }
                }
                Token::Any => {
                    if let Some(first) = current.iter().position(|reached| *reached) {
                        next[first..].iter_mut().for_each(|reached| *reached = true);
                    }
                }
                Token::Directories => {
                    if let Some(first) = current.iter().position(|reached| *reached) {
                        next.clone_from(&current);
                        for p in first..n {
                            if path[p] == '/' {
                                next[p + 1] = true;
                            }
                        }
                    }
                }
            }
            current = next;
        }
        (0..=n).any(|p| current[p] && (p == n || (!whole && path[p] == '/')))
    }
}
