use clap::Parser;

use serde::Deserialize;
use serde::Serialize;

use std::fs::File;
use std::path::Path;
use std::io::Write;
use std::io::BufReader;

use bytes::Bytes;

use reqwest::header::AUTHORIZATION;

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

    /// Email
    #[arg(long, default_value = "")]
    email: String,

    /// Password
    #[arg(long, default_value = "")]
    password: String,
}

#[derive(Serialize, Debug, Deserialize)]
pub struct Credentials {
    email: String,
    password: String,
}

#[derive(Deserialize, Debug)]
struct Edition {
    id: i32,
    slug: String,
    pdf: String
}

#[derive(Serialize, Deserialize, Debug)]
struct User {
    userId: i32,
    email: String,
    membershipCode: String,
    firstName: String,
    lastName: String
}

#[derive(Serialize, Deserialize, Debug)]
struct Token {
    expiresIn: i32,
    accessToken: String,
    refreshToken: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct Login {
    user: User,
    token: Token
}


#[derive(Deserialize, Debug)]
struct Post {
    id: i32,
    slug: String,
    title: String,
    link: String
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

    // Build the client using the builder pattern
    let client = reqwest::Client::builder().cookie_store(true).build()?;

    // Perform the actual execution of the network request
    let res = client
        .get(&format!("{}/wp/editions/latest", BASE_URL))
        //.headers(headers)
        .send()
        .await?;

    // Parse the response body as Json in this case
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
        let req_str = format!(r#"{{"refreshToken":"{}"}}"#, login.token.refreshToken);
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
        let credentials = Credentials{email: args.email, password: args.password};

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


    let auth_code = format!("Bearer {}", login.token.accessToken);

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
        for post in posts.data {
            // TODO: make this a function

            let res = client
                .get(&format!("{}/wp/posts/{}/download/epub", BASE_URL, post.slug))
                .header(AUTHORIZATION, reqwest::header::HeaderValue::from_str(&auth_code).unwrap())
                .send()
                .await?;

            let filename = format!("{}.epub", post.slug);
            let content =  res.bytes().await?;
            write_file(filename, content)?;

        }
    }

    Ok(())
}

