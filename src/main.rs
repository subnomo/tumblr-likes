use clap::{crate_version, App, Arg};
use indicatif::ProgressBar;
use regex::Regex;
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};

mod types;
mod util;

use crate::types::*;
use crate::util::*;

#[derive(Debug)]
pub struct Arguments {
    api_key: String,
    blog_name: String,
    directory: String,
    dump: Option<String>,
    restore: Option<String>,
    export: Option<String>,
    verbose: bool,
}

fn cli() -> Arguments {
    let env_key = env::var("TUMBLR_API_KEY");

    let matches = App::new("tumblr-likes")
        .version(crate_version!())
        .author("Alex Taylor <alex@alext.xyz>")
        .about("Downloads your liked photos and videos on Tumblr.")
        .arg(
            Arg::with_name("API_KEY")
                .short('a')
                .help("Your Tumblr API key")
                .takes_value(true)
                .required(env_key.is_err())
                .conflicts_with("JSON_RESTORE"),
        )
        .arg(
            Arg::with_name("BLOG_NAME")
                .short('b')
                .help("The blog to download likes from")
                .takes_value(true)
                .required(true)
                .conflicts_with("JSON_RESTORE"),
        )
        .arg(
            Arg::with_name("OUTPUT_DIR")
                .short('d')
                .long("dir")
                .help("The download directory")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("JSON_DUMP")
                .long("dump")
                .help("Dumps liked posts into the given JSON file")
                .takes_value(true)
                .conflicts_with("JSON_RESTORE"),
        )
        .arg(
            Arg::with_name("JSON_RESTORE")
                .long("restore")
                .help("Restores liked posts from given JSON file")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("HTML_FILE")
                .long("export")
                .short('e')
                .help("Exports liked posts into the given HTML file")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("verbose")
                .short('v')
                .long("verbose")
                .help("Prints extra information, used for debugging"),
        )
        .get_matches();

    Arguments {
        api_key: match matches.value_of("API_KEY") {
            Some(a) => a.to_string(),
            None => {
                if !env_key.is_err() {
                    env_key.unwrap().to_string()
                } else {
                    "".to_string()
                }
            }
        },

        blog_name: match matches.value_of("BLOG_NAME") {
            Some(b) => b.to_string(),
            None => "".to_string(),
        },

        directory: match matches.value_of("OUTPUT_DIR") {
            Some(d) => d.to_string(),
            None => "downloads".to_string(),
        },

        dump: matches.value_of("JSON_DUMP").map(|s| s.to_string()),
        restore: matches.value_of("JSON_RESTORE").map(|s| s.to_string()),
        export: matches.value_of("HTML_FILE").map(|s| s.to_string()),
        verbose: matches.is_present("verbose"),
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = cli();
    let client = reqwest::Client::new();
    let bar;
    let mut all_posts: Vec<Post> = Vec::new();
    let mut files: Vec<Vec<Option<PathBuf>>> = Vec::new();

    if !args.restore.is_none() {
        if args.verbose {
            println!("Restoring dump...");
        }

        let posts = restore_dump(args.restore.clone().unwrap())?;
        bar = ProgressBar::new(posts.len() as _);

        // If not exporting, just do a download
        if args.export.is_none() {
            if args.verbose {
                println!("Downloading posts...");
            }

            files = download_posts(posts, &client, &args, &bar).await?;
        } else {
            all_posts = posts;
        }
    } else {
        let info_url = build_url(&args, true, None);

        if args.verbose {
            println!("Info URL: {}", info_url);
        }

        let info = client.get(&info_url).send().await?;

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

        let info: ReturnVal = info.json().await?;

        if args.verbose {
            println!("Info: {:#?}", info);
        }

        bar = ProgressBar::new(info.response.liked_count as _);

        // Setup directory if not in export mode
        if args.export.is_none() {
            setup_directory(&args);
        }

        // Do rip
        let mut before = None;

        if args.verbose {
            println!("Downloading likes...");
        }

        loop {
            let url = build_url(&args, false, before);

            let mut res: ReturnVal = client.get(&url).send().await?.json().await?;
            let links = res.response._links;

            if !args.dump.is_none() || !args.export.is_none() {
                // If dumping or exporting, we need to collect every post
                all_posts.append(&mut res.response.liked_posts);
            } else {
                files.append(
                    &mut download_posts(res.response.liked_posts, &client, &args, &bar).await?,
                );
            }

            if let Some(l) = links {
                before = if let Some(next) = l.next {
                    Some(next.query_params.before)
                } else {
                    break;
                };
            } else {
                break;
            }
        }
    }

    // Dump
    if let Some(dump_file) = args.dump {
        dump(all_posts, dump_file);
        return Ok(());
    }

    // Export
    if let Some(export_file) = args.export {
        export(&client, all_posts, export_file, &bar, args.verbose).await;
        bar.finish();
        return Ok(());
    }

    // Rename files with index

    if args.verbose {
        println!("Renaming files...\n");
    }

    rename(files);
    bar.finish();

    Ok(())
}

async fn download_posts(
    posts: Vec<Post>,
    client: &reqwest::Client,
    args: &Arguments,
    bar: &ProgressBar,
) -> Result<Vec<Vec<Option<PathBuf>>>, Box<dyn Error>> {
    let mut files: Vec<Vec<Option<PathBuf>>> = Vec::new();

    for post in posts {
        let mut post_files: Vec<Option<PathBuf>> = Vec::new();

        if post.kind == "photo" {
            if let Some(photos) = post.photos {
                for photo in photos {
                    post_files.push(download(client, args, "pics", photo.original_size.url).await?);
                }
            }
        } else if post.kind == "video" {
            if let Some(url) = post.video_url {
                post_files.push(download(&client, &args, "videos", url).await?);
            }
        }

        files.push(post_files);
        bar.inc(1);
    }

    Ok(files)
}

fn rename(files: Vec<Vec<Option<PathBuf>>>) {
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
}

fn dump(posts: Vec<Post>, file: String) {
    let path = Path::new(&file);
    let display = path.display();

    let mut file = match File::create(&path) {
        Ok(f) => f,
        Err(e) => panic!("Couldn't create file {}: {}", display, e),
    };

    let json = serde_json::to_string(&posts).unwrap();

    match file.write_all(json.as_bytes()) {
        Ok(_) => println!("Dumped liked post data to {}.", display),
        Err(e) => panic!("Couldn't write to {}: {}", display, e),
    }
}

fn restore_dump(file: String) -> Result<Vec<Post>, Box<dyn Error>> {
    let path = Path::new(&file);
    let file = File::open(path)?;
    let reader = BufReader::new(file);

    let res: Vec<Post> = serde_json::from_reader(reader)?;
    Ok(res)
}

static HTML_TEMPLATE: &'static str = "<!DOCTYPE html>
<html lang='en'>
<head>
    <meta charset='UTF-8'>
    <meta name='viewport' content='width=device-width, initial-scale=1'>
    <title>Tumblr Likes</title>
    <link rel='stylesheet' href='https://cdnjs.cloudflare.com/ajax/libs/bulma/0.7.2/css/bulma.min.css'>
    <style>
        .container {
            max-width: 625px;
        }

        .card {
            margin-top: 20px;
            margin-bottom: 20px;
        }
    </style>
</head>
<body>
    <div class='container'>
        {{cards}}
    </div>
</body>
</html>
";

static CARD_TEMPLATE: &'static str = "<div class='card'>
    <div class='card-header'>
        <div class='card-header-title'>
            {{title}}
        </div>
    </div>

    <div class='card-content'>
        <div class='content'>
            {{body}}
        </div>
        {{tags}}
        <div class='tags'>
            <span class='tag'>{{date}}</span>
            <span class='tag'>{{note_count}} notes</span>
        </div>
    </div>
</div>
";

async fn export(
    client: &reqwest::Client,
    posts: Vec<Post>,
    file: String,
    bar: &ProgressBar,
    verbose: bool,
) {
    // Create export directory
    fs::create_dir_all("export").expect("Could not create export directory!");

    if verbose {
        println!("Exporting your liked posts...");
    }

    let mut posts_html = String::new();

    for post in posts {
        let title = format!("<a href='{}'>{}</a>", post.post_url, post.blog_name);
        let mut card = CARD_TEMPLATE.replace("{{title}}", &title);

        if post.tags.len() > 0 {
            let tags = format!(
                "<div class='tags'><span class='tag'>{}</span></div>",
                post.tags.join("</span><span class='tag'>")
            );
            card = card.replace("{{tags}}", &tags);
        } else {
            card = card.replace("{{tags}}", "");
        }

        card = card.replace("{{date}}", &post.date);
        card = card.replace("{{note_count}}", &post.note_count.to_string());

        if post.kind == "text" {
            if let Some(body) = post.body {
                let mut content = body.clone();

                // Extract URLs from body
                let re = Regex::new(r#"src="([^"]+)"#).unwrap();
                let caps = re.captures_iter(&body);

                // Replace all objects with locally stored ones
                for cap in caps {
                    let url = cap.get(1).unwrap().as_str().to_string();
                    let split: Vec<&str> = url.split("/").collect();
                    let filename = split.last().unwrap();

                    let dl =
                        download_url(&client, url.clone(), format!("export/{}", filename)).await;

                    content = content.replace(
                        &url,
                        &inject_content(dl, "Could not fetch object", |path| {
                            let src = path.to_str().unwrap();
                            src.to_string()
                        }),
                    );
                }

                card = card.replace("{{body}}", &content);
                posts_html = format!("{}{}", posts_html, card);
            }
        } else if post.kind == "video" {
            let mut body = String::new();

            if let Some(trail) = post.trail {
                let mut trail_content = render_trail(trail);

                // Inject video
                if let Some(url) = post.video_url {
                    let split: Vec<&str> = url.split("/").collect();
                    let filename = split.last().unwrap();

                    let dl =
                        download_url(&client, url.clone(), format!("export/{}", filename)).await;

                    trail_content = trail_content.replace(
                        "{{content}}",
                        &inject_content(dl, "Could not fetch video", |path| {
                            let src = path.to_str().unwrap();
                            let video = format!(
                                "<p><figure><video controls='controls' autoplay='autoplay' \
                                 muted='muted'><source src='{}'></video></figure></p>",
                                src
                            );

                            video
                        }),
                    );
                }

                trail_content = trail_content.replace("{{content}}", "");
                body = trail_content;
            }

            card = card.replace("{{body}}", &body);
            posts_html = format!("{}{}", posts_html, card);
        } else if post.kind == "photo" {
            let mut body = String::new();

            if let Some(trail) = post.trail {
                let mut trail_content = render_trail(trail);

                // Inject photos
                if let Some(photos) = post.photos {
                    for photo in photos {
                        let url = photo.original_size.url;
                        let split: Vec<&str> = url.split("/").collect();
                        let filename = split.last().unwrap();
                        let dl = download_url(&client, url.clone(), format!("export/{}", filename))
                            .await;

                        trail_content = trail_content.replace(
                            "{{content}}",
                            &inject_content(dl, "Could not fetch photo", |path| {
                                let src = path.to_str().unwrap();
                                let img = format!(
                                    "<figure><img src='{}' /></figure>{{{{content}}}}",
                                    src
                                );

                                img
                            }),
                        );
                    }
                }

                trail_content = trail_content.replace("{{content}}", "");
                body = trail_content;
            }

            card = card.replace("{{body}}", &body);
            posts_html = format!("{}{}", posts_html, card);
        }

        bar.inc(1);
    }

    // Write to html file
    let out = HTML_TEMPLATE.replace("{{cards}}", &posts_html);

    let path = Path::new(&file);
    let display = path.display();

    let mut file = match File::create(&path) {
        Ok(f) => f,
        Err(e) => panic!("Couldn't create file {}: {}", display, e),
    };

    match file.write_all(out.as_bytes()) {
        Ok(_) => {
            if verbose {
                println!("Exported liked posts to {}.", display)
            }
        }
        Err(e) => panic!("Couldn't write to {}: {}", display, e),
    }
}
