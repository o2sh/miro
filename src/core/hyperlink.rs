use failure::{ensure, err_msg, Error};
use regex::{Captures, Regex};
use serde::{self, Deserialize, Deserializer};
use serde_derive::*;
use std::collections::HashMap;
use std::fmt::{Display, Error as FmtError, Formatter};
use std::ops::Range;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hyperlink {
    params: HashMap<String, String>,
    uri: String,

    implicit: bool,
}

impl Hyperlink {
    pub fn uri(&self) -> &str {
        &self.uri
    }

    #[inline]
    pub fn is_implicit(&self) -> bool {
        self.implicit
    }

    pub fn new_implicit<S: Into<String>>(uri: S) -> Self {
        Self { uri: uri.into(), params: HashMap::new(), implicit: true }
    }

    pub fn new_with_params<S: Into<String>>(uri: S, params: HashMap<String, String>) -> Self {
        Self { uri: uri.into(), params, implicit: false }
    }

    pub fn parse(osc: &[&[u8]]) -> Result<Option<Hyperlink>, Error> {
        ensure!(osc.len() == 3, "wrong param count");
        if osc[1].is_empty() && osc[2].is_empty() {
            Ok(None)
        } else {
            let param_str = String::from_utf8(osc[1].to_vec())?;
            let uri = String::from_utf8(osc[2].to_vec())?;

            let mut params = HashMap::new();
            if !param_str.is_empty() {
                for pair in param_str.split(':') {
                    let mut iter = pair.splitn(2, '=');
                    let key = iter.next().ok_or_else(|| err_msg("bad params"))?;
                    let value = iter.next().ok_or_else(|| err_msg("bad params"))?;
                    params.insert(key.to_owned(), value.to_owned());
                }
            }

            Ok(Some(Hyperlink::new_with_params(uri, params)))
        }
    }
}

impl Display for Hyperlink {
    fn fmt(&self, f: &mut Formatter) -> Result<(), FmtError> {
        write!(f, "8;")?;
        for (idx, (k, v)) in self.params.iter().enumerate() {
            if idx > 0 {
                write!(f, ":")?;
            }
            write!(f, "{}={}", k, v)?;
        }

        write!(f, ";{}", self.uri)?;

        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Rule {
    #[serde(deserialize_with = "deserialize_regex")]
    regex: Regex,

    format: String,
}

fn deserialize_regex<'de, D>(deserializer: D) -> Result<Regex, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Regex::new(&s).map_err(|e| serde::de::Error::custom(format!("{:?}", e)))
}

#[derive(Debug, PartialEq)]
pub struct RuleMatch {
    pub range: Range<usize>,

    pub link: Arc<Hyperlink>,
}

struct Match<'t> {
    rule: &'t Rule,
    captures: Captures<'t>,
}

impl<'t> Match<'t> {
    fn len(&self) -> usize {
        let c0 = self.captures.get(0).unwrap();
        c0.end() - c0.start()
    }

    fn range(&self) -> Range<usize> {
        let c0 = self.captures.get(0).unwrap();
        c0.start()..c0.end()
    }

    fn expand(&self) -> String {
        let mut result = self.rule.format.clone();

        for n in (0..self.captures.len()).rev() {
            let search = format!("${}", n);
            result = result.replace(&search, self.captures.get(n).unwrap().as_str());
        }
        result
    }
}

impl Rule {
    pub fn new(regex: &str, format: &str) -> Result<Self, Error> {
        Ok(Self { regex: Regex::new(regex)?, format: format.to_owned() })
    }

    pub fn match_hyperlinks(line: &str, rules: &[Rule]) -> Vec<RuleMatch> {
        let mut matches = Vec::new();
        for rule in rules.iter() {
            for captures in rule.regex.captures_iter(line) {
                matches.push(Match { rule, captures });
            }
        }

        matches.sort_by(|a, b| b.len().cmp(&a.len()));

        matches
            .into_iter()
            .map(|m| {
                let url = m.expand();
                let link = Arc::new(Hyperlink::new_implicit(url));
                RuleMatch { link, range: m.range() }
            })
            .collect()
    }
}
