// CLI argument parsing
use clap::Parser;

// JSON
use serde::{Deserialize, Serialize};

// Files
use bytes::Bytes;
use std::fs::{create_dir, remove_dir_all, File};
use std::io::{BufReader, Cursor, ErrorKind, Seek, Write};
use std::path::{Path, PathBuf};

// HTTP requests and URL parsing
use reqwest::header::AUTHORIZATION;
use url::Url;

// Epubs
use epub::doc::EpubDoc;
use epub_builder::{EpubBuilder, EpubContent, EpubVersion, ReferenceType, Result, ZipLibrary};

use random_string::generate;

// Image
use image::io::Reader as ImageReader;
use image::ImageFormat;

// Logging
use log::{info, warn};

// API base URL
pub const BASE_URL: &str = "https://api.ilmanifesto.it/api/v1";

// The set of characters below is used to generate random names for the pictures
// and can be removed once I check that epub_builder generates
// valid ids from file names.
// When doing that be careful, sometime figures are reused and thus have the same name.
// The correct way is probably using uniquely defined names (md5?).
pub const CHARSET: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

// Hacky way to have a templete for format
macro_rules! IMAGE_HTML {
    () => {
        r#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml">
  <head>
    <title>{title}</title>
  </head>
  <body>
    <{tsize}>{title}</{tsize}>
    <img src="{imgurl}" />
    {summary}
  </body>
</html>
"#
    };
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Downloads PDF version
    #[arg(short, long, default_value_t = false)]
    pdf: bool,

    /// Downloads ePUB files
    #[arg(short, long, default_value_t = false)]
    epub: bool,

    /// Generates a single ePUB file
    #[arg(short, long, default_value_t = false)]
    single_epub: bool,

    /// Keep downloaded ePUB files (mainly for debugging)
    #[arg(short, long, default_value_t = false)]
    keep_files: bool,

    /// Email
    #[arg(long, default_value = "")]
    email: String,

    /// Password
    #[arg(long, default_value = "")]
    password: String,
}

#[derive(Serialize, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Credentials {
    email: String,
    password: String,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct Edition {
    id: i32,
    slug: String,
    pdf: String,
    title: String,
    featured_image: Option<Image>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct User {
    user_id: i32,
    email: String,
    membership_code: String,
    first_name: String,
    last_name: String,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct Token {
    expires_in: i32,
    access_token: String,
    refresh_token: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct Login {
    user: User,
    token: Token,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct Post {
    // id: i32, Not currently used
    slug: String,
    title: String,
    kicker: String,
    summary: String,
    excerpt: String,
    // link: String, Not currently used
    cover_position: Option<i32>,
    cover_summary: String,
    cover_title: String,
    cover_image: Option<Image>,
    featured_image: Option<Image>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct Image {
    #[allow(unused_variables)]
    src: String,
    // alt: String Not currently used
}

#[derive(Deserialize, Debug)]
struct Data {
    data: Vec<Post>,
}

fn write_file(filename: String, content: Bytes, is_tmp: bool) -> std::io::Result<()> {
    let path;
    let tmp_file = std::env::temp_dir().join("clima-rs").join(&filename);

    if is_tmp {
        // create if missing
        if let Err(e) = create_dir(tmp_file.parent().unwrap()) {
            if e.kind() != ErrorKind::AlreadyExists {
                return Err(e);
            }
        }

        path = tmp_file.as_path();
    } else {
        path = Path::new(&filename);
    }

    let mut file = match File::create(&path) {
        Err(why) => panic!("Couldn't create {}", why),
        Ok(file) => file,
    };
    file.write_all(&content)?;
    Ok(())
}

fn extract_file_from_url(url_str: &String) -> Result<String, Box<dyn std::error::Error>> {
    let url = Url::parse(&url_str)?;
    let path_segments = url.path_segments().ok_or_else(|| "cannot be base")?;
    Ok(String::from(path_segments.last().unwrap()))
}

fn resize_image(image_path: PathBuf) -> Result<Cursor<Vec<u8>>, Box<dyn std::error::Error>> {
    let reader = ImageReader::open(image_path).expect("Failed reading image");
    let img = reader.decode().expect("Damn I couldn't decode the picture");

    let mut buff = Cursor::new(Vec::new());

    img.thumbnail(600, 600)
        .write_to(&mut buff, ImageFormat::Jpeg)?;
    buff.rewind().unwrap();
    Ok(buff)
}

fn combine_articles(
    edition: Edition,
    posts: Data,
    keep_files: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    // Create a new EpubBuilder using the zip library
    let mut builder = EpubBuilder::new(ZipLibrary::new()?)?;

    // Set some metadata
    builder.add_author("il Manifesto");
    builder.set_title(&edition.title);
    builder.set_lang("it");
    builder.set_toc_name(&edition.title);
    builder.epub_version(EpubVersion::V30);
    //builder.set_publication_date Maybe in the future

    let tmp_dir = std::env::temp_dir().join("clima-rs");

    // Add cover
    let cover_file = &format!("{}.jpg", edition.slug);
    let cover_path = tmp_dir.join(cover_file);
    if cover_path.is_file() {
        builder.add_cover_image("cover.jpg", File::open(cover_path).unwrap(), "image/jpeg")?;
    }

    builder.inline_toc();

    let mut posts_data = posts.data;
    // sort by cover position (on None, set to 99)
    posts_data.sort_by_key(|element| element.cover_position.or(Some(99)));

    // add cover page
    for post in &posts_data {
        if post.cover_image.is_some() {
            let image_url = &post.cover_image.as_ref().unwrap().src;
            let image_name = extract_file_from_url(image_url)?;
            let image_path = tmp_dir.join(&image_name);

            // convert and resize to small jpegs
            if Path::new(&image_path).exists() {
                let unique_image_name = format!("{}.jpg", generate(12, CHARSET));

                // file name is used in the id of xml file and cannot start with number
                builder.add_resource(
                    &unique_image_name,
                    resize_image(image_path)?,
                    "image/jpeg",
                )?;

                let title_file = format!("{}-cover.xhtml", post.slug);
                let title_content = format!(
                    IMAGE_HTML!(),
                    tsize = "h1",
                    title = post.cover_title,
                    imgurl = unique_image_name,
                    summary = post.cover_summary
                );

                builder.add_content(
                    EpubContent::new(title_file, title_content.as_bytes())
                        .reftype(ReferenceType::Text),
                )?;
            }
        }
    }

    for post in &posts_data {
        // add image to archive
        if post.featured_image.is_some() {
            let image_url = &post.featured_image.as_ref().unwrap().src;
            let image_name = extract_file_from_url(image_url)?;
            let image_path = tmp_dir.join(&image_name);

            // convert and resize to small jpegs

            if Path::new(&image_path).exists() {
                let unique_image_name = format!("{}.jpg", generate(12, CHARSET));

                // file name is used in the id of xml file and cannot start with number
                builder.add_resource(
                    &unique_image_name,
                    resize_image(image_path)?,
                    "image/jpeg",
                )?;

                let title_file = format!("{}-front.xhtml", post.slug);
                let title_content = format!(
                    IMAGE_HTML!(),
                    tsize = "h4",
                    title = if post.kicker.is_empty() {
                        &post.title
                    } else {
                        &post.kicker
                    },
                    imgurl = unique_image_name,
                    summary = post.excerpt
                );

                builder.add_content(
                    EpubContent::new(title_file, title_content.as_bytes())
                        .reftype(ReferenceType::Text),
                )?;
            }
        }

        // Files are stored in temp directory in this case
        let filename = tmp_dir.join(&format!("{}.epub", post.slug));

        //let filename = format!("{}.epub", post.slug);
        let doc = EpubDoc::new(&filename);
        if !doc.is_ok() {
            continue;
        };

        let mut doc = doc.unwrap();

        assert_eq!("application/xhtml+xml", doc.get_current_mime().unwrap());

        let content = doc
            .get_resource_str_by_path("OEBPS/Chapter001.xhtml")
            .unwrap();

        let content_file = format!("{}.xhtml", post.slug);

        // Add a chapter, mark it as beginning of the "real content"
        builder.add_content(
            EpubContent::new(
                content_file,
                content
                    .replace("h0", "h1")
                    .replace("quote", "blockquote")
                    .as_bytes(),
            )
            .title(&post.title)
            .reftype(ReferenceType::Text), //.level(2)
        )?;
    }

    // Use standard file writer?
    let filename = format!("{}.epub", &edition.slug);
    let f = File::create(&filename).expect("Unable to create file");
    builder.generate(f)?;

    // Keep epub files if requested
    if !keep_files {
        remove_dir_all(std::env::temp_dir().join("clima-rs"))?;
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // Create the client
    let client = reqwest::Client::builder().cookie_store(true).build()?;

    // Get last edition (currently the only supported operation)
    let res = client
        .get(&format!("{}/wp/editions/latest", BASE_URL))
        //.headers(headers)
        .send()
        .await?;

    // Parse the response body
    let edition = res.json::<Edition>().await?;

    info!("{:?}", edition.slug);

    // Check if token is already available, or ask it to server
    let mut login: Login;
    if Path::new("login.json").is_file() {
        let file = File::open("login.json")?;
        let reader = BufReader::new(file);

        // Read token saved with first login
        login = serde_json::from_reader(reader)?;

        // refresh token (if required, but we do it every time now)
        let req_str = format!(r#"{{"refreshToken":"{}"}}"#, login.token.refresh_token);
        let json_req: serde_json::Value = serde_json::from_str(&req_str).unwrap();

        let res = client
            .post(&format!("{}/auth/token", BASE_URL))
            .json(&json_req)
            .send()
            .await?;

        // update token part
        login.token = res.json::<Token>().await?;

        // save it again
        let file = match File::create("login.json") {
            Err(why) => panic!("couldn't create {}", why),
            Ok(file) => file,
        };
        serde_json::to_writer(&file, &login)?;
    } else {
        // obtain credentials from CLI or stored locally in credentials.json
        let credentials;

        if args.email.is_empty() || args.password.is_empty() {
            if Path::new("credentials.json").is_file() {
                let file = File::open("credentials.json")?;
                let reader = BufReader::new(file);

                // Read credentials from file
                credentials = serde_json::from_reader(reader)?;
            } else {
                panic!("Credentials required!");
            }
        } else {
            credentials = Credentials {
                email: args.email,
                password: args.password,
            };
        }

        let res = client
            .post(&format!("{}/auth/login", BASE_URL))
            .json(&credentials)
            .send()
            .await?;
        login = res.json::<Login>().await?;

        let file = match File::create("login.json") {
            Err(why) => panic!("couldn't create {}", why),
            Ok(file) => file,
        };
        serde_json::to_writer(&file, &login)?;

        info!("{:?}", login);
    }

    let auth_code = format!("Bearer {}", login.token.access_token);

    // Download PDF
    if args.pdf {
        info!(
            "{:?}",
            &format!("{}/wp/pdfs/slug/{}/download", BASE_URL, edition.pdf)
        );
        let res = client
            .get(&format!(
                "{}/wp/pdfs/slug/{}/download",
                BASE_URL, edition.pdf
            ))
            .header(
                AUTHORIZATION,
                reqwest::header::HeaderValue::from_str(&auth_code).unwrap(),
            )
            .send()
            .await?;

        let filename = format!("{}.pdf", edition.slug);
        let content = res.bytes().await?;
        write_file(filename, content, false)?;
    }

    // Download EPUB and images
    if args.epub {
        // Download cover image for this edition
        if edition.featured_image.is_some() {
            let res = client
                .get(&edition.featured_image.as_ref().unwrap().src)
                .send()
                .await?;

            let content = res.bytes().await?;
            let filename = format!("{}.jpg", edition.slug);
            write_file(filename, content, args.single_epub)?;
        }
        // Download cover image for this edition
        if edition.featured_image.is_some() {
            let res = client
                .get(&edition.featured_image.as_ref().unwrap().src)
                .send()
                .await?;

            let content = res.bytes().await?;
            let filename = format!("{}.jpg", edition.slug);
            write_file(filename, content, args.single_epub)?;
        }

        let res = client
            .get(&format!("{}/wp/editions/{}/posts", BASE_URL, edition.id))
            .header(
                AUTHORIZATION,
                reqwest::header::HeaderValue::from_str(&auth_code).unwrap(),
            )
            .send()
            .await?;

        // parse posts
        let posts = res.json::<Data>().await?;

        for post in &posts.data {
            let filename = format!("{}.epub", post.slug);

            if std::env::temp_dir()
                .join("clima-rs")
                .join(&filename)
                .exists()
            {
                continue;
            };

            let res = client
                .get(&format!(
                    "{}/wp/posts/{}/download/epub",
                    BASE_URL, post.slug
                ))
                .header(
                    AUTHORIZATION,
                    reqwest::header::HeaderValue::from_str(&auth_code).unwrap(),
                )
                .send()
                .await?;

            let content = res.bytes().await?;
            write_file(filename, content, args.single_epub)?;

            // Download cover image for main article
            if post.cover_image.is_some() {
                let image_url = &post.cover_image.as_ref().unwrap().src;
                let image_name = extract_file_from_url(&image_url)?;
                let res = client
                    .get(image_url.as_str())
                    .header(
                        AUTHORIZATION,
                        reqwest::header::HeaderValue::from_str(&auth_code).unwrap(),
                    )
                    .send()
                    .await?;

                let content = res.bytes().await?;
                write_file(image_name, content, args.single_epub)?;
            }

            // Download cover image for each article
            if post.featured_image.is_some() {
                let image_url = &post.featured_image.as_ref().unwrap().src;
                let image_name = extract_file_from_url(&image_url)?;

                let res = client
                    .get(image_url.as_str())
                    .header(
                        AUTHORIZATION,
                        reqwest::header::HeaderValue::from_str(&auth_code).unwrap(),
                    )
                    .send()
                    .await?;

                let content = res.bytes().await?;
                write_file(image_name, content, args.single_epub)?;
            }
        }

        // Creates a single output file
        if args.single_epub {
            combine_articles(edition, posts, args.keep_files)?;
        }
    }

    Ok(())
}
