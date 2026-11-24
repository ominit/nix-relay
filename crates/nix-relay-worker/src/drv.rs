use std::collections::{BTreeMap, BTreeSet};

use color_eyre::eyre::{Result, WrapErr, bail, eyre};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Derivation {
    pub outputs: BTreeMap<String, String>,
    pub input_drvs: BTreeMap<String, BTreeSet<String>>,
    pub system: String,
    pub env: BTreeMap<String, String>,
}

impl Derivation {
    pub async fn read(path: &str) -> Result<Self> {
        let bytes = tokio::fs::read(path)
            .await
            .wrap_err_with(|| format!("reading {path}"))?;
        parse(&bytes).wrap_err_with(|| format!("parsing {path}"))
    }

    pub fn required_features(&self) -> Vec<String> {
        match self.structured_attrs() {
            Some(attrs) => attrs
                .get("requiredSystemFeatures")
                .and_then(serde_json::Value::as_array)
                .map(|features| {
                    features
                        .iter()
                        .filter_map(|f| f.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default(),
            None => self
                .env
                .get("requiredSystemFeatures")
                .map(|features| features.split_whitespace().map(str::to_owned).collect())
                .unwrap_or_default(),
        }
    }

    pub fn prefer_local_build(&self) -> bool {
        match self.structured_attrs() {
            Some(attrs) => attrs
                .get("preferLocalBuild")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            None => self.env.get("preferLocalBuild").is_some_and(|v| v == "1"),
        }
    }

    fn structured_attrs(&self) -> Option<serde_json::Map<String, serde_json::Value>> {
        serde_json::from_str(self.env.get("__json")?).ok()
    }
}

pub fn parse(input: &[u8]) -> Result<Derivation> {
    let mut p = Parser { input, pos: 0 };
    if !p.eat(b"Derive(") {
        bail!("unsupported derivation format");
    }
    let outputs = p.list(|p| {
        p.expect(b"(")?;
        let name = p.string()?;
        p.expect(b",")?;
        let path = p.string()?;
        p.expect(b",")?;
        p.string()?;
        p.expect(b",")?;
        p.string()?;
        p.expect(b")")?;
        Ok((name, path))
    })?;
    p.expect(b",")?;
    let input_drvs = p.list(|p| {
        p.expect(b"(")?;
        let path = p.string()?;
        p.expect(b",")?;
        let outputs = p.list(Parser::string)?;
        p.expect(b")")?;
        Ok((path, outputs.into_iter().collect()))
    })?;
    p.expect(b",")?;
    p.list(Parser::string)?;
    p.expect(b",")?;
    let system = p.string()?;
    p.expect(b",")?;
    p.string()?;
    p.expect(b",")?;
    p.list(Parser::string)?;
    p.expect(b",")?;
    let env = p.list(|p| {
        p.expect(b"(")?;
        let key = p.string()?;
        p.expect(b",")?;
        let value = p.string()?;
        p.expect(b")")?;
        Ok((key, value))
    })?;
    p.expect(b")")?;
    if p.pos != input.len() {
        bail!("trailing data after derivation at byte {}", p.pos);
    }
    Ok(Derivation {
        outputs: outputs.into_iter().collect(),
        input_drvs: input_drvs.into_iter().collect(),
        system,
        env: env.into_iter().collect(),
    })
}

struct Parser<'a> {
    input: &'a [u8],
    pos: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.input.get(self.pos).copied()
    }

    fn eat(&mut self, token: &[u8]) -> bool {
        let matches = self
            .input
            .get(self.pos..)
            .is_some_and(|rest| rest.starts_with(token));
        if matches {
            self.pos += token.len();
        }
        matches
    }

    fn expect(&mut self, token: &[u8]) -> Result<()> {
        if self.eat(token) {
            Ok(())
        } else {
            Err(eyre!(
                "expected {:?} at byte {}",
                String::from_utf8_lossy(token),
                self.pos
            ))
        }
    }

    fn list<T>(&mut self, mut item: impl FnMut(&mut Self) -> Result<T>) -> Result<Vec<T>> {
        self.expect(b"[")?;
        let mut items = Vec::new();
        if self.eat(b"]") {
            return Ok(items);
        }
        loop {
            items.push(item(self)?);
            if self.eat(b"]") {
                return Ok(items);
            }
            self.expect(b",")?;
        }
    }

    fn string(&mut self) -> Result<String> {
        self.expect(b"\"")?;
        let mut bytes = Vec::new();
        loop {
            let byte = self
                .peek()
                .ok_or_else(|| eyre!("unterminated string at byte {}", self.pos))?;
            self.pos += 1;
            match byte {
                b'"' => break,
                b'\\' => {
                    let escaped = self
                        .peek()
                        .ok_or_else(|| eyre!("unterminated escape at byte {}", self.pos))?;
                    self.pos += 1;
                    bytes.push(match escaped {
                        b'n' => b'\n',
                        b'r' => b'\r',
                        b't' => b'\t',
                        other => other,
                    });
                }
                other => bytes.push(other),
            }
        }
        Ok(String::from_utf8(bytes)
            .unwrap_or_else(|error| String::from_utf8_lossy(error.as_bytes()).into_owned()))
    }
}

#[cfg(test)]
mod tests;
