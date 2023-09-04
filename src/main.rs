use clap::Parser;

use serde::Deserialize;
use serde::Serialize;

use std::fs::File;
use std::path::Path;
use std::io::Write;
use std::io::BufReader;
use std::fs;

use bytes::Bytes;
use bytes::buf::BufExt;

use reqwest::header::AUTHORIZATION;


use epub_builder::EpubBuilder;
use epub_builder::Result;
use epub_builder::ZipLibrary;
use epub_builder::EpubContent;
use epub_builder::ReferenceType;

use epub::doc::EpubDoc;



pub const BASE_URL: &str = "https://api.ilmanifesto.it/api/v1";


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
    featured_image: Option<Image>
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct User {
    user_id: i32,
    email: String,
    membership_code: String,
    first_name: String,
    last_name: String
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
    token: Token
}


#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct Post {
    // id: i32, Not currently used
    slug: String,
    title: String,
    // link: String, Not currently used
    cover_position: Option<i32>
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
    data: Vec<Post>
}

fn write_file(filename: String, content: Bytes) -> std::io::Result<()> {
    println!("Saving to {filename}");
    let path = Path::new(&filename);

    let mut file = match File::create(&path) {
        Err(why) => panic!("couldn't create {}", why),
        Ok(file) => file,
    };
    file.write_all(&content)?;
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
    let edition = res
        .json::<Edition>()
        .await?;

    println!("{:?}", edition.slug);

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
        login.token = res.json::<Token>()
                    .await?;
        // save it again
        let file = match File::create("login.json") {
            Err(why) => panic!("couldn't create {}", why),
            Ok(file) => file,
        };
        serde_json::to_writer(&file, &login)?;

    } else {

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
            credentials = Credentials{email: args.email, password: args.password};
        }

        let res = client
            .post(&format!("{}/auth/login", BASE_URL))
            .json(&credentials)
            .send()
            .await?;
        login = res.json::<Login>()
                    .await?;

        let file = match File::create("login.json") {
            Err(why) => panic!("couldn't create {}", why),
            Ok(file) => file,
        };
        serde_json::to_writer(&file, &login)?;

        println!("{:?}", login);
    }


    let auth_code = format!("Bearer {}", login.token.access_token);

    if args.pdf {
        println!("{:?}", &format!("{}/wp/pdfs/slug/{}/download", BASE_URL, edition.pdf));
        let res = client
            .get(&format!("{}/wp/pdfs/slug/{}/download", BASE_URL, edition.pdf))
            .header(AUTHORIZATION, reqwest::header::HeaderValue::from_str(&auth_code).unwrap())
            .send()
            .await?;

        let filename = format!("{}.pdf", edition.slug);
        let content =  res.bytes().await?;
        write_file(filename, content)?;
    }

    if args.epub {
        let res = client
            .get(&format!("{}/wp/editions/{}/posts", BASE_URL, edition.id))
            .header(AUTHORIZATION, reqwest::header::HeaderValue::from_str(&auth_code).unwrap())
            .send()
            .await?;

        let posts = res.json::<Data>()
                    .await?;

        for post in &posts.data {
            // TODO: make this a function
            let filename = format!("{}.epub", post.slug);

            if Path::new(&filename).exists() { continue };

            let res = client
                .get(&format!("{}/wp/posts/{}/download/epub", BASE_URL, post.slug))
                .header(AUTHORIZATION, reqwest::header::HeaderValue::from_str(&auth_code).unwrap())
                .send()
                .await?;

            let content =  res.bytes().await?;
            write_file(filename, content)?;

        }

        // Creates a single output file
        if args.single_epub {
            // TODO: make this a function

            // Create a new EpubBuilder using the zip library
            let mut builder = EpubBuilder::new(ZipLibrary::new()?)?;

            // Set some metadata
            builder.add_author("il Manifesto");
            builder.set_title(edition.title);
            builder.set_lang("it");
            builder.set_toc_name("Articoli");
            //builder.set_publication_date Maybe in the future

            // Grab image and place it as cover
            if edition.featured_image.is_some() {
                let res = client
                    .get(&edition.featured_image.unwrap().src)
                    .send()
                    .await?;

                let content =  res.bytes().await?;
                //let filename = format!("{}.jpg", edition.slug);
                //write_file(filename, content)?;
                builder.add_cover_image("cover.jpg", content.reader(), "image/jpeg")?;
            }

            builder.inline_toc();

            let mut posts_data = posts.data;

            // sort by cover position (on None, set to 99)
            posts_data.sort_by_key(|element| element.cover_position.or(Some(99)));

            for post in &posts_data {

                let filename = format!("{}.epub", post.slug);
                let doc = EpubDoc::new(&filename);
                if !doc.is_ok() { continue };

                let mut doc = doc.unwrap();

                assert_eq!("application/xhtml+xml", doc.get_current_mime().unwrap());

                //let title = doc.metadata.get("title").unwrap();
                // println!("Title: {0}", &title[0]);

                let content = doc.get_resource_str_by_path("OEBPS/Chapter001.xhtml").unwrap();
                let content_file = format!("{}.xhtml", post.slug);

                // Add a chapter, mark it as beginning of the "real content"
                builder.add_content(
                    EpubContent::new(content_file, content.as_bytes())
                        .title(&post.title)
                        .reftype(ReferenceType::Text),
                )?;

                // Keep epub files if requested
                if !args.keep_files { fs::remove_file(&filename)?; }
            }


            // Use standard file writer?
            let filename = format!("{}.epub", edition.slug);
            let f = File::create(&filename).expect("Unable to create file");
            builder.generate(f)?;
        }
    }

    Ok(())
}

