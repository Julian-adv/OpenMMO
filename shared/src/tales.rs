use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Deed {
    pub date: String,
    pub hero: String,
    pub brief: String,
}

impl Deed {
    pub fn parse(line: &str) -> Option<Self> {
        let mut fields = line.splitn(3, '|').map(str::trim);
        let date = fields.next()?;
        let hero = fields.next()?;
        let brief = fields.next()?;
        if date.is_empty() || hero.is_empty() || brief.is_empty() {
            return None;
        }
        Some(Self {
            date: date.to_string(),
            hero: hero.to_string(),
            brief: brief.to_string(),
        })
    }
}
