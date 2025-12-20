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
impl fmt::Display for Quality {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.slug(), self.file_extension())
    }
}

pub trait FileExtension {
    fn file_extension(&self) -> &str;
}
impl FileExtension for Quality {
    fn file_extension(&self) -> &str {
        match self {
            Quality::WebpMaxres => "webp",
            Quality::JpgMaxres => "jpg",
            Quality::WebpSd => "webp",
            Quality::JpgSd => "jpg",
            Quality::WebpHq => "webp",
            Quality::JpgHq => "jpg",
        }
    }
}

pub trait Slug {
    fn slug(&self) -> &str;
}
impl Slug for Quality {
    fn slug(&self) -> &str {
        match self {
            Quality::WebpMaxres => "maxresdefault",
            Quality::JpgMaxres => "maxresdefault",
            Quality::WebpSd => "sddefault",
            Quality::JpgSd => "sddefault",
            Quality::WebpHq => "hqdefault",
            Quality::JpgHq => "hqdefault",
        }
    }
}
