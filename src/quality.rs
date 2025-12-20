use std::fmt;

#[derive(Debug, PartialEq)]
pub enum Quality {
    WebpMaxres,
    JpgMaxres,
    WebpSd,
    JpgSd,
    WebpHq,
    JpgHq,
}

impl Quality {
    pub fn file_extension(&self) -> &str {
        match self {
            Quality::WebpMaxres | Quality::WebpSd | Quality::WebpHq => "webp",
            Quality::JpgMaxres | Quality::JpgSd | Quality::JpgHq => "jpg",
        }
    }

    pub fn slug(&self) -> &str {
        match self {
            Quality::WebpMaxres | Quality::JpgMaxres => "maxresdefault",
            Quality::WebpSd | Quality::JpgSd => "sddefault",
            Quality::WebpHq | Quality::JpgHq => "hqdefault",
        }
    }

    pub fn from_s3_key(key: &str) -> Option<Quality> {
        let parts = key.split('.').collect::<Vec<&str>>();
        if parts.len() != 3 {
            return None;
        }
        let slug = parts[1];
        let file_extension = parts[2];
        match file_extension {
            "webp" => match slug {
                "maxresdefault" => Some(Quality::WebpMaxres),
                "sddefault" => Some(Quality::WebpSd),
                "hqdefault" => Some(Quality::WebpHq),
                _ => None,
            },
            "jpg" => match slug {
                "maxresdefault" => Some(Quality::JpgMaxres),
                "sddefault" => Some(Quality::JpgSd),
                "hqdefault" => Some(Quality::JpgHq),
                _ => None,
            },
            _ => None,
        }
    }
}

impl fmt::Display for Quality {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.slug(), self.file_extension())
    }
}
