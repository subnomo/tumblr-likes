#[macro_use]
extern crate serde_derive;
extern crate serde_json;

extern crate indicatif;
extern crate reqwest;
extern crate serde;
extern crate toml;

mod types;

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use toml::from_str;
use indicatif::ProgressBar;
use types::ReturnVal;


#[derive(Debug, Deserialize)]
struct Credentials {
    api_key: String,
    blog_name: String,
}

#[cfg(windows)]
const LINE_ENDING: &'static str = "\r\n";
#[cfg(not(windows))]
const LINE_ENDING: &'static str = "\n";

fn get_credentials() -> Result<Credentials, String> {
    // Parse credentials.toml
    let mut file = match File::open("credentials.toml") {
        Ok(f) => f,
        Err(_) => {
            let mut f = File::create("credentials.toml")
                .expect("Could not create credentials.toml!");

            let contents = format!("api_key = \"\"{}blog_name = \"\"", LINE_ENDING);
            f.write(contents.as_bytes()).expect("Could not write to credentials.toml!");

            return Err("Please fill out \"credentials.toml\" with your Tumblr credentials.".to_string());
        },
    };

    let mut file_text = String::new();

    file.read_to_string(&mut file_text).unwrap_or_else(|e| {
        panic!("Could not read credentials.toml! Error: {}", e);
    });

    let bad_cred = "Could not parse credentials.toml! Did you fill it out correctly?".to_string();

    let cred: Credentials = match from_str(file_text.as_str()) {
        Ok(c) => c,
        Err(_) => {
            return Err(bad_cred);
        },
    };

    if cred.api_key == "" || cred.blog_name == "" {
        return Err(bad_cred);
    }

    Ok(cred)
}

fn build_url(cred: &Credentials, one: bool, before: Option<String>) -> String {
    let limit = if one {
        1
    } else {
        20
    };

    let before = match before {
        Some(b) => format!("&before={}", b),
        _ => "".to_string(),
    };

    format!("https://api.tumblr.com/v2/blog/{}/likes?api_key={}&limit={}{}", cred.blog_name,
        cred.api_key, limit, before)
}

fn setup_directory() {
    fs::create_dir_all("downloads/pics")
        .expect("Could not create download directory!");

    fs::create_dir_all("downloads/videos")
        .expect("Could not create download directory!");
}

fn exists(folder: String, name: String) -> bool {
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

fn download(client: &reqwest::Client, folder: &str, url: String) -> Result<Option<PathBuf>, reqwest::Error> {
    let split: Vec<&str> = url.split("/").collect();
    let filename = split.last().unwrap();
    let folder = format!("downloads/{}", folder);
    let file = format!("{}/{}", folder, filename);
    let path = Path::new(&file);

    // Skip already downloaded files
    if exists(folder, filename.to_string()) {
        return Ok(None);
    }

    let mut res = client.get(&url)
        .send()?;

    if res.status().is_success() {
        let mut f = File::create(path)
            .expect("Could not create file!");

        std::io::copy(&mut res, &mut f)
            .expect("Could not download file!");

        return Ok(Some(path.to_path_buf()));
    }

    Ok(None)
}

fn main() -> Result<(), reqwest::Error> {
    let c = match get_credentials() {
        Ok(cred) => cred,
        Err(err) => {
            println!("{}", err);
            return Ok(());
        }
    };

    let client = reqwest::Client::new();

    let info_url = build_url(&c, true, None);
    let mut info = client.get(&info_url)
        .send()?;

    if !info.status().is_success() {
        println!("There was an error fetching your likes. Is there a mistake in credentials.toml?");
        return Ok(());
    }

    let info: ReturnVal = info.json()?;
    let bar = ProgressBar::new(info.response.liked_count as _);

    setup_directory();

    // Do rip
    let mut before = None;
    let mut files: Vec<Vec<PathBuf>> = Vec::new();

    loop {
        let url = build_url(&c, false, before.clone());

        let res: ReturnVal = client.get(&url)
            .send()?
            .json()?;

        let _links = res.response._links;

        for post in res.response.liked_posts {
            let mut post_files: Vec<PathBuf> = Vec::new();

            if post.kind == "photo" {
                if let Some(photos) = post.photos {
                    for photo in photos {
                        match download(&client, "pics", photo.original_size.url)? {
                            Some(path) => post_files.push(path),
                            _ => {},
                        };
                    }
                }
            } else if post.kind == "video" {
                if let Some(url) = post.video_url {
                    match download(&client, "videos", url)? {
                        Some(path) => post_files.push(path),
                        _ => {},
                    };
                }
            }

            if post_files.len() > 0 {
                files.push(post_files);
            }

            bar.inc(1);
        }

        if let Some(links) = _links {
            before = Some(links.next.query_params.before);
        } else {
            break;
        }
    }

    // Rename files with index
    for (i, post) in files.iter().rev().enumerate() {
        for file in post {
            let filename = &file.file_name()
                .unwrap()
                .to_str()
                .unwrap();

            let mut new_file = file.clone();
            new_file.set_file_name(format!("{} - {}", i + 1, filename));

            fs::rename(&file, new_file).unwrap_or_else(|e| {
                panic!("Could not rename file! Error: {}", e);
            });
        }
    }

    bar.finish();

    Ok(())
}
