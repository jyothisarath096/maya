#![allow(dead_code)]

use alloc::string::String;

pub fn normalize(path: &str) -> String {
    let mut normalized = String::from("/");
    let mut first = true;

    for component in path.split('/').filter(|part| !part.is_empty()) {
        if !first {
            normalized.push('/');
        }
        normalized.push_str(component);
        first = false;
    }

    if normalized.len() > 1 && normalized.ends_with('/') {
        normalized.pop();
    }

    normalized
}

pub fn parent(path: &str) -> &str {
    if path == "/" {
        return "/";
    }

    let trimmed = path.trim_end_matches('/');
    match trimmed.rfind('/') {
        Some(0) | None => "/",
        Some(index) => &trimmed[..index],
    }
}

pub fn filename(path: &str) -> &str {
    if path == "/" {
        return "";
    }

    let trimmed = path.trim_end_matches('/');
    match trimmed.rfind('/') {
        Some(index) => &trimmed[index + 1..],
        None => trimmed,
    }
}
