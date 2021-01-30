use std::fmt::Display;
use std::str::FromStr;

use chrono::TimeZone;
use serde::{Deserialize, Deserializer, Serialize};
use serde::de::Error;
use scraper::ElementRef;

fn from_str<'de, T: FromStr, D: Deserializer<'de>>(deserializer: D) -> Result<T, D::Error> where T::Err: Display {
    T::from_str(&String::deserialize(deserializer)?).map_err(D::Error::custom)
}

macro_rules! maperr {
    ($value: expr, $err: literal) => {
        match $value {
            Ok(result) => result,
            Err(e) => {
                return Err(format!("{:?}: {:?}", e, $err))
            }
        }
    };
}

#[derive(Deserialize, Serialize, Debug)]
struct Footer {
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    icon_url: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct Image {
    url: String
}

#[derive(Serialize, Deserialize, Debug)]
struct Embed {
    title: String,
    #[serde(rename = "type")]
    ty: String,
    description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
    timestamp: String,
    footer: Footer,
    image: Image,
}

#[derive(Serialize, Deserialize, Debug)]
struct Webhook {
    content: String,
    username: String,
    avatar_url: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    embeds: Vec<Embed>,
    #[serde(skip)]
    send_to: &'static str,
}

#[derive(Serialize, Deserialize, Debug)]
struct Xkcd {
    #[serde(deserialize_with = "from_str")]
    month: u32,
    num: i32,
    link: String,
    #[serde(deserialize_with = "from_str")]
    year: i32,
    news: String,
    safe_title: String,
    transcript: String,
    alt: String,
    img: String,
    title: String,
    #[serde(deserialize_with = "from_str")]
    day: u32,
}

#[derive(Debug)]
struct Qc {
    title: String,
    link: String,
    img: String,
    date: String,
}

impl Webhook {
    async fn send(self, client: &reqwest::Client) -> Result<(), String> {
        maperr!(client.post(self.send_to).json(&self).send().await, "reqwest error");
        Ok(())
    }
}

impl Xkcd {
    async fn get(client: &reqwest::Client, index: Option<i32>) -> Result<Xkcd, String> {
        Ok(maperr!(
            serde_json::from_value(maperr!(
                serde_json::Value::from_str(&maperr!(maperr!(
                    client.get(&match index {
                        Some(index) => format!("https://xkcd.com/{}/info.0.json", index),
                        None => "https://xkcd.com/info.0.json".to_string()
                    }).send().await,
                    "reqwest error").text().await, "reqwest error")),
                "serde_json error")),
            "serde_json error")
        )
    }
}

impl Into<Webhook> for Xkcd {
    fn into(self) -> Webhook {
        Webhook {
            content: "".to_string(),
            username: "ComicCron xkcd".to_string(),
            avatar_url: AVATAR_URL.to_string(),
            embeds: vec![Embed {
                title: format!("#{}: {}", self.num, self.title),
                ty: "rich".to_string(),
                description: "".to_string(),
                url: Some(self.link),
                timestamp: format!("{}", chrono::Utc.ymd(self.year, self.month, self.day).and_hms(0, 0, 0).format("%+")),
                footer: Footer { text: self.alt, icon_url: Some("https://cdn.discordapp.com/attachments/751998036841857099/804483113001812028/919f27-2.png".to_string()) },
                image: Image { url: self.img },
            }],
            send_to: XKCD,
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct ComicCronState {
    last_posted_xkcd: i32
}

impl ComicCronState {
    async fn get() -> Result<ComicCronState, String> {
        Ok(maperr!(
            serde_json::from_value(maperr!(
                serde_json::Value::from_str(&maperr!(std::fs::read_to_string("comic_cron.json"), "std::fs error")),
            "serde_json error")),
        "serde_json error"))
    }
    fn set(self) -> Result<(), String> {
        maperr!(std::fs::write("comic_cron.json", maperr!(serde_json::to_string(&self), "serde_json error")), "std::fs error");
        Ok(())
    }
}

const XKCD: &'static str = "https://discord.com/api/webhooks/804475443620216872/KkSwxUkd5_7HQusYFWXrJRUS3n5YcBBNjDvIZenaKLH2WIk4YwGV0Q5-sV1d_Ix_JsTa";
const AVATAR_URL: &'static str = "https://cdn.discordapp.com/attachments/751998036841857099/804504521215705118/zoey_pink_twitter.jpg";
// const SMBC: &'static str = "https://discord.com/api/webhooks/804475468886573056/56f6C3rIKIZa28TmXwKkt4ivOQg1rzePD2ZME4j3ZppLzQGLe15SFb2bDOo5sW6jHHlo";
// const QC: &'static str = "https://discord.com/api/webhooks/804475489720860712/akQZ8PXXCvf0Av9774LY6fp5Itlnquo7GsiPfT49P7DG8RJ8JW2lyefjpc_5fgBwfmEB";

fn main() {
    let x: Result<(), String> = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let client = reqwest::Client::new();
            let res = maperr!(maperr!(client.get("https://www.questionablecontent.net/QCRSS.xml").send().await, "reqwest error").text().await, "reqwest error");
            let doc = scraper::Html::parse_document(&res);
            let sel = scraper::Selector::parse("item:first-child").unwrap();
            let res = doc.select(&sel).next().unwrap();
            fn visit(x: ElementRef) {
                println!("[{:?}]", x.html());
                for y in x.children() {
                    let node = y.value();
                    if let Some(e) = node.as_element() {
                        visit(ElementRef::wrap(y).unwrap());
                    }
                }
            }
            visit(res);

            // let pkg = maperr!(sxd_document::parser::parse(&res), "sxd_document error");
            // let doc = pkg.as_document();
            // let title = sxd_xpath::evaluate_xpath(&doc, "/rss/channel/item[1]/title").unwrap().string();
            // let link = sxd_xpath::evaluate_xpath(&doc, "/rss/channel/item[1]/link").unwrap().string();
            // let description_text = sxd_xpath::evaluate_xpath(&doc, "/rss/channel/item[1]/description").unwrap().string();
            // let date = sxd_xpath::evaluate_xpath(&doc, "/rss/channel/item[1]/pubDate").unwrap().string();
            // println!("{}\n{}\n[{}]\n{}\n", title, link, description_text, date);
            // let desc_pkg = maperr!(sxd_document::parser::parse(&format!("<root>{}</root>", description_text)), "sxd_document error");
            // let desc_doc = desc_pkg.as_document();
            // let img_url = sxd_xpath::evaluate_xpath(&desc_doc, "img");
            // println!("{:?}", img_url);

            // let state = ComicCronState::get().await?;
            // let webhook: Webhook = Xkcd::get(&client, Some(state.last_posted_xkcd)).await?.into();
            // webhook.send(&client).await?;
            // state.set()?;
            Ok(())
        });
    match x {
        Ok(_) => println!("Success!"),
        Err(err) => println!("Error: {}", err)
    }
}