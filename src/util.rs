use std::fs::{self, File};
use std::path::{Path, PathBuf};

use crate::types::TrailItem;
use crate::Arguments;

pub fn build_url(args: &Arguments, one: bool, before: Option<String>) -> String {
    let limit = if one { 1 } else { 20 };

    let before = match before {
        Some(b) => format!("&before={}", b),
        _ => "".to_string(),
    };

    format!(
        "https://api.tumblr.com/v2/blog/{}/likes?api_key={}&limit={}{}",
        args.blog_name, args.api_key, limit, before
    )
}

pub fn setup_directory(args: &Arguments) {
    fs::create_dir_all(format!("{}/pics", args.directory))
        .expect("Could not create download directory!");

    fs::create_dir_all(format!("{}/videos", args.directory))
        .expect("Could not create download directory!");
}

pub fn exists(folder: String, name: String) -> bool {
    // Check if file containing name exists
    for file in fs::read_dir(folder).unwrap() {
        let file = file.unwrap().path();
        let filename = match file.to_str() {
            Some(s) => s.to_string(),
            _ => continue,
        };

        if filename.contains(&name) {
            return true;
        }
    }

    false
}

type DownloadResult = Result<Option<PathBuf>, reqwest::Error>;

pub async fn download_url(client: &reqwest::Client, url: String, file: String) -> DownloadResult {
    let path = Path::new(&file);

    // Skip existing files
    if path.exists() {
        return Ok(Some(path.to_path_buf()));
    }

    let res = client.get(&url).send().await?;

    if res.status().is_success() {
        let mut f = File::create(path).expect("Could not create file!");
        std::io::copy(&mut res.bytes().await?.as_ref(), &mut f).expect("Could not download file!");

        return Ok(Some(path.to_path_buf()));
    }

    Ok(None)
}

pub async fn download(
    client: &reqwest::Client,
    args: &Arguments,
    folder: &str,
    url: String,
) -> Result<Option<PathBuf>, reqwest::Error> {
    let split: Vec<&str> = url.split("/").collect();
    let filename = split.last().unwrap();
    let folder = format!("{}/{}", args.directory, folder);
    let file = format!("{}/{}", folder, filename);

    // Skip already downloaded files
    if exists(folder, filename.to_string()) {
        return Ok(None);
    }

    download_url(&client, url.clone(), file).await
}

pub fn render_trail(trail: Vec<TrailItem>) -> String {
    let mut trail_content = "{{content}}".to_string();

    for item in trail.iter().rev() {
        let blog = &item.blog.name;
        let id = &item.post.id;
        let content = &item.content_raw;

        let url = format!("https://{}.tumblr.com/post/{}/", blog, id);
        let name = format!("<p><a href='{}'>{}</a>:</p>", url, blog);

        let combined = format!(
            "{}<blockquote>{{{{content}}}}{}</blockquote>",
            name, content
        );

        trail_content = trail_content.replace("{{content}}", &combined);
    }

    trail_content
}

pub fn inject_content<F: Fn(PathBuf) -> String>(
    raw: DownloadResult,
    error_text: &str,
    cb: F,
) -> String {
    return match raw {
        Ok(p) => match p {
            Some(path) => cb(path),
            None => error_text.to_string(),
        },

        _ => error_text.to_string(),
    };
}
