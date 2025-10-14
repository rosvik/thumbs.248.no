use std::{fmt, path::PathBuf};

#[derive(Debug, PartialEq)]
pub enum Quality {
    WebpMaxres,
    WebpSd,
    JpgHq,
}
impl fmt::Display for Quality {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.slug(), self.file_extension())
    }
}

pub trait PathName {
    fn path_name(&self) -> PathBuf;
}
impl PathName for Quality {
    fn path_name(&self) -> PathBuf {
        PathBuf::from(format!("{}/{}", self.slug(), self.file_extension()))
    }
}

pub trait FileExtension {
    fn file_extension(&self) -> &str;
}
impl FileExtension for Quality {
    fn file_extension(&self) -> &str {
        match self {
            Quality::WebpMaxres => "webp",
            Quality::WebpSd => "webp",
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
            Quality::WebpSd => "sddefault",
            Quality::JpgHq => "hqdefault",
        }
    }
}
