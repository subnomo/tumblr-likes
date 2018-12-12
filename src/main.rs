#[macro_use]
extern crate serde_derive;
extern crate serde_json;

extern crate clap;
extern crate indicatif;
extern crate reqwest;
extern crate serde;

mod types;

use clap::{App, Arg};
use indicatif::ProgressBar;
use std::env;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use types::ReturnVal;

#[derive(Debug)]
struct Arguments {
    api_key: String,
    blog_name: String,
    directory: String,
    verbose: bool,
}

fn build_url(args: &Arguments, one: bool, before: Option<String>) -> String {
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

fn setup_directory(args: &Arguments) {
    fs::create_dir_all(format!("{}/pics", args.directory))
        .expect("Could not create download directory!");

    fs::create_dir_all(format!("{}/videos", args.directory))
        .expect("Could not create download directory!");
}

fn exists(folder: String, name: String) -> bool {
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

fn download(
    client: &reqwest::Client,
    args: &Arguments,
    folder: &str,
    url: String,
) -> Result<Option<PathBuf>, reqwest::Error> {
    let split: Vec<&str> = url.split("/").collect();
    let filename = split.last().unwrap();
    let folder = format!("{}/{}", args.directory, folder);
    let file = format!("{}/{}", folder, filename);
    let path = Path::new(&file);

    // Skip already downloaded files
    if exists(folder, filename.to_string()) {
        return Ok(None);
    }

    let mut res = client.get(&url).send()?;

    if res.status().is_success() {
        let mut f = File::create(path).expect("Could not create file!");
        std::io::copy(&mut res, &mut f).expect("Could not download file!");

        return Ok(Some(path.to_path_buf()));
    }

    Ok(None)
}

fn cli() -> Arguments {
    let env_key = env::var("TUMBLR_API_KEY");

    let matches = App::new("tumblr-likes")
        .version("0.2.3")
        .author("Alex Taylor <alex@alext.xyz>")
        .about("Downloads your liked photos and videos on Tumblr.")
        .arg(
            Arg::with_name("API_KEY")
                .short("a")
                .help("Your Tumblr API key")
                .takes_value(true)
                .required(env_key.is_err()),
        )
        .arg(
            Arg::with_name("BLOG_NAME")
                .short("b")
                .help("The blog to download likes from")
                .takes_value(true)
                .required(true),
        )
        .arg(
            Arg::with_name("OUTPUT_DIR")
                .short("d")
                .long("dir")
                .help("The download directory")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("verbose")
                .short("v")
                .long("verbose")
                .help("Prints extra information, used for debugging"),
        )
        .get_matches();

    Arguments {
        api_key: match matches.value_of("API_KEY") {
            Some(a) => a.to_string(),
            None => env_key.unwrap().to_string(),
        },

        blog_name: match matches.value_of("BLOG_NAME") {
            Some(b) => b.to_string(),
            None => "".to_string(),
        },

        directory: match matches.value_of("OUTPUT_DIR") {
            Some(d) => d.to_string(),
            None => "downloads".to_string(),
        },

        verbose: matches.is_present("verbose"),
    }
}

fn main() -> Result<(), reqwest::Error> {
    let args = cli();

    let client = reqwest::Client::new();
    let info_url = build_url(&args, true, None);

    if args.verbose {
        println!("Info URL: {}", info_url);
    }

    let mut info = client.get(&info_url).send()?;

    if args.verbose {
        println!("{:#?}", info);
    }

    if !info.status().is_success() {
        println!(
            "There was an error fetching your likes. Please make sure \
             you provided the correct API key and blog name."
        );
        return Ok(());
    }

    let info: ReturnVal = info.json()?;

    if args.verbose {
        println!("Info: {:#?}", info);
    }

    let bar = ProgressBar::new(info.response.liked_count as _);

    setup_directory(&args);

    // Do rip
    let mut before = None;
    let mut files: Vec<Vec<Option<PathBuf>>> = Vec::new();

    if args.verbose {
        println!("Downloading likes...");
    }

    loop {
        let url = build_url(&args, false, before.clone());

        let res: ReturnVal = client.get(&url).send()?.json()?;
        let _links = res.response._links;

        for post in res.response.liked_posts {
            let mut post_files: Vec<Option<PathBuf>> = Vec::new();

            if post.kind == "photo" {
                if let Some(photos) = post.photos {
                    for photo in photos {
                        post_files.push(download(&client, &args, "pics", photo.original_size.url)?);
                    }
                }
            } else if post.kind == "video" {
                if let Some(url) = post.video_url {
                    post_files.push(download(&client, &args, "videos", url)?);
                }
            }

            files.push(post_files);
            bar.inc(1);
        }

        if let Some(links) = _links {
            before = Some(links.next.query_params.before);
        } else {
            break;
        }
    }

    // Rename files with index

    if args.verbose {
        println!("Renaming files...\n");
    }

    for (i, post) in files.iter().rev().enumerate() {
        for file in post {
            if let Some(file) = file {
                let filename = &file.file_name().unwrap().to_str().unwrap();

                let mut new_file = file.clone();
                new_file.set_file_name(format!("{} - {}", i + 1, filename));

                fs::rename(&file, new_file).unwrap_or_else(|e| {
                    panic!("Could not rename file! Error: {}", e);
                });
            }
        }
    }

    bar.finish();

    Ok(())
}
