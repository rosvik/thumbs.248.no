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
}

impl fmt::Display for Quality {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.slug(), self.file_extension())
    }
}
