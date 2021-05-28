use std::fmt::Display;
use std::str::FromStr;

use chrono::TimeZone;
use macky_xml::{Node, QuerySupport};
use reqwest::Client;
use serde::{Deserialize, Deserializer, Serialize};
use serde::de::Error;

fn from_str<'de, T: FromStr, D: Deserializer<'de>>(deserializer: D) -> Result<T, D::Error> where T::Err: Display {
    T::from_str(&String::deserialize(deserializer)?).map_err(D::Error::custom)
}

#[derive(Serialize, Deserialize, Debug)]
struct Field {
    name: String,
    value: String,
    inline: bool,
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
    fields: Vec<Field>,
    footer: Option<Footer>,
    image: Option<Image>,
}

#[derive(Serialize, Deserialize, Debug)]
struct Webhook {
    content: String,
    username: String,
    avatar_url: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    embeds: Vec<Embed>,
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
struct RssItem {
    title: String,
    link: String,
    img_url: String,
    alt_text: String,
    pub_date: String,
    guid: String,
}

impl RssItem {
    fn qc_webhook(&self, potential_skip: bool) -> Option<Webhook> {
        self.webhook(potential_skip, "QC", "https://www.questionablecontent.net/favicon/favicon-16x16.png")
    }

    fn smbc_webhook(&self, potential_skip: bool) -> Option<Webhook> {
        self.webhook(potential_skip, "SMBC", "https://www.smbc-comics.com/favicon.ico")
    }

    fn webhook(&self, potential_skip: bool, embed_title: &'static str, footer: &'static str) -> Option<Webhook> {
        Some(Webhook {
            content: if potential_skip { "Some items may have been skipped" } else { "" }.to_string(),
            username: format!("ComicCron {}", embed_title),
            avatar_url: AVATAR_URL.to_string(),
            embeds: vec![Embed {
                title: format!("{}", self.title),
                ty: "rich".to_string(),
                description: "".to_string(),
                url: Some(self.link.to_owned()),
                timestamp: chrono::DateTime::parse_from_rfc2822(&self.pub_date).ok()?.format("%+").to_string(),
                fields: vec![],
                footer: Some(Footer { text: self.alt_text.to_owned(), icon_url: Some(footer.to_string()) }),
                image: Some(Image { url: self.img_url.to_owned() }),
            }],
        })
    }

    fn parse_qc_desc(data: Vec<&Node>) -> Option<(&str, &str)> {
        let img_url = data.elem_name("img").first()?.attributes.get("src")?;
        Some((img_url, ""))
    }

    fn parse_smbc_desc(data: Vec<&Node>) -> Option<(&str, &str)> {
        let img_url = data.elem_name("img").first()?.attributes.get("src")?;
        Some((img_url, ""))
    }

    fn from_rss(item: &macky_xml::Element, description: impl Fn(Vec<&Node>) -> Option<(&str, &str)>) -> Option<RssItem> {
        let title = item.children().elem_name("title").only()?.children().only()?.as_cdata()?;
        let link = item.children().elem_name("link").only()?.children().only()?.as_cdata()?;
        let description_text = item.children().elem_name("description").only()?.children().only()?.as_cdata()?;
        let description_parser = macky_xml::Parser {
            allow_no_close: vec!["img".to_string(), "!doctype".to_string()]
        };
        let description_text = format!("<root>{}</root>", description_text);
        let description_doc = description_parser.complete_element(&description_text)?;
        let (img_url, alt_text) = description(description_doc.children())?;
        let pub_date = item.children().elem_name("pubDate").only()?.children().only()?.as_cdata()?.to_owned();
        let guid = item.children().elem_name("guid").only()?.children().only()?.as_cdata()?.to_owned();

        Some(RssItem {
            title: title.to_string(),
            link: link.to_string(),
            img_url: img_url.to_string(),
            alt_text: alt_text.to_string(),
            pub_date,
            guid,
        })
    }
}

impl Webhook {
    async fn send(self, client: &reqwest::Client, webhooks: &Vec<String>) -> Result<(), String> {
        for url in webhooks {
            client.post(url).json(&self).send().await.map_err(|_| "error sending webhook".to_string())?;
        }
        Ok(())
    }
    fn debug(fields: Vec<Field>) -> Webhook {
        Webhook {
            content: "".to_string(),
            username: "ComicCron Debug".to_string(),
            avatar_url: AVATAR_URL.to_string(),
            embeds: vec![Embed {
                title: "".to_string(),
                ty: "rich".to_string(),
                description: "".to_string(),
                url: None,
                timestamp: "".to_string(),
                fields,
                footer: None,
                image: None,
            }],
        }
    }
}

impl Xkcd {
    async fn get(client: &reqwest::Client, index: Option<i32>) -> Result<Xkcd, String> {
        let url = match index {
            Some(index) => format!("https://xkcd.com/{}/info.0.json", index),
            None => "https://xkcd.com/info.0.json".to_string()
        };
        let response = client.get(&url).send().await.map_err(|_| format!("url -> request {:?}", index))?;
        let text = response.text().await.map_err(|_| format!("request -> text: #{:?}", index))?;
        let json = serde_json::Value::from_str(&text).map_err(|_| format!("text -> json {:?}", index))?;
        serde_json::from_value(json).map_err(|_| format!("json -> rust {:?}", index))
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
                fields: vec![],
                footer: Some(Footer { text: self.alt, icon_url: Some("https://cdn.discordapp.com/attachments/751998036841857099/804483113001812028/919f27-2.png".to_string()) }),
                image: Some(Image { url: self.img }),
            }],
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct ComicCronState {
    xkcd: i32,
    qc: String,
    smbc: String,
    xkcd_webhooks: Vec<String>,
    qc_webhooks: Vec<String>,
    smbc_webhooks: Vec<String>,
    debug_webhooks: Vec<String>,
}

impl ComicCronState {
    fn get() -> Result<ComicCronState, String> {
        let text = std::fs::read_to_string("comic_cron.json").map_err(|_| "filesystem -> text".to_string())?;
        let json = serde_json::Value::from_str(&text).map_err(|_| "text -> json".to_string())?;
        serde_json::from_value(json).map_err(|_| "json -> rust".to_string())
    }
    fn set(&self) -> Result<(), String> {
        let text = serde_json::to_string_pretty(&self).map_err(|_| "rust -> text".to_string())?;
        std::fs::write("comic_cron.json", text).map_err(|_| "text -> filesystem".to_string())
    }
}

const AVATAR_URL: &'static str = "https://cdn.discordapp.com/attachments/751998036841857099/804504521215705118/zoey_pink_twitter.jpg";

type Success = Option<String>;

async fn xkcd(client: &Client, state: &mut ComicCronState) -> Result<Success, String> {
    let latest_xkcd = Xkcd::get(client, None).await?;
    let post = if state.xkcd + 1 == latest_xkcd.num {
        latest_xkcd
    } else if state.xkcd + 1 < latest_xkcd.num {
        Xkcd::get(client, Some(state.xkcd + 1)).await?
    } else {
        return Ok(None);
    };
    let num = post.num.to_string();
    let webhook: Webhook = post.into();
    webhook.send(client, &state.xkcd_webhooks).await?;
    state.xkcd += 1;
    Ok(Some(num))
}

async fn qc(client: &Client, state: &mut ComicCronState) -> Result<Success, String> {
    let response = client.get("https://www.questionablecontent.net/QCRSS.xml").send().await.map_err(|_| "url -> request".to_string())?;
    let text = response.text().await.map_err(|_| "request -> text".to_string())?;
    let document = macky_xml::Parser::default().complete_document(&text).ok_or("text -> xml".to_string())?;
    let xml_items = document.root.children().elem_name("item");
    if xml_items.len() == 0 {
        Err("no rss items".to_string())
    } else {
        let mut rss_items = vec![];
        for xml in xml_items {
            rss_items.push(RssItem::from_rss(xml, RssItem::parse_qc_desc).ok_or("xml -> rust")?);
        }
        if rss_items[0].guid == state.qc {
            Ok(None)
        } else {
            for i in 1..rss_items.len() {
                if rss_items[i].guid == state.qc {
                    let webhook = rss_items[i - 1].qc_webhook(false).ok_or("rust -> webhook".to_string())?;
                    webhook.send(client, &state.qc_webhooks).await?;
                    state.qc = rss_items[i - 1].guid.to_string();
                    return Ok(Some(rss_items[i - 1].title.to_string()));
                }
            }
            let webhook = rss_items[rss_items.len() - 1].qc_webhook(true).ok_or("rust -> webhook".to_string())?;
            webhook.send(client, &state.qc_webhooks).await?;
            state.qc = rss_items[rss_items.len() - 1].guid.to_string();
            Ok(Some(rss_items[rss_items.len() - 1].title.to_string()))
        }
    }
}

async fn smbc(client: &Client, state: &mut ComicCronState) -> Result<Success, String> {
    let response = client.get("https://www.smbc-comics.com/comic/rss").send().await.map_err(|_| "url -> request".to_string())?;
    let text = response.text().await.map_err(|_| "request -> text".to_string())?;
    let document = macky_xml::Parser::default().complete_document(&text).ok_or("text -> xml".to_string())?;
    let xml_items = document.root.children().elem_name("item");
    if xml_items.len() == 0 {
        Err("no rss items".to_string())
    } else {
        let mut rss_items = vec![];
        for xml in xml_items {
            rss_items.push(RssItem::from_rss(xml, RssItem::parse_smbc_desc).ok_or("xml -> rust")?);
        }
        if rss_items[0].guid == state.smbc {
            Ok(None)
        } else {
            for i in 1..rss_items.len() {
                if rss_items[i].guid == state.smbc {
                    let webhook = rss_items[i - 1].smbc_webhook(false).ok_or("rust -> webhook".to_string())?;
                    webhook.send(client, &state.smbc_webhooks).await?;
                    state.smbc = rss_items[i - 1].guid.to_string();
                    return Ok(Some(rss_items[i - 1].title.to_string()));
                }
            }
            let webhook = rss_items[rss_items.len() - 1].smbc_webhook(true).ok_or("rust -> webhook".to_string())?;
            webhook.send(client, &state.smbc_webhooks).await?;
            state.smbc = rss_items[rss_items.len() - 1].guid.to_string();
            Ok(Some(rss_items[rss_items.len() - 1].title.to_string()))
        }
    }
}

fn main() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            match ComicCronState::get() {
                Ok(mut state) => {
                    let client = reqwest::Client::new();
                    let xkcd = format!("{:?}", xkcd(&client, &mut state).await);
                    let qc = format!("{:?}", qc(&client, &mut state).await);
                    let smbc = format!("{:?}", smbc(&client, &mut state).await);
                    let save = format!("{:?}", state.set());

                   if let Err(err) = Webhook::debug(vec![
                        Field {
                            name: "xkcd".to_string(),
                            value: format!("`{}`", xkcd),
                            inline: false,
                        },
                        Field {
                            name: "QC".to_string(),
                            value: format!("`{}`", qc),
                            inline: false,
                        },
                        Field {
                            name: "SMBC".to_string(),
                            value: format!("`{}`", smbc),
                            inline: false,
                        },
                        Field {
                            name: "Save".to_string(),
                            value: format!("`{}`", save),
                            inline: false,
                        }
                    ]).send(&client, &state.debug_webhooks).await {
                        println!("xkcd: {:?}", xkcd);
                        println!("qc  : {:?}", qc);
                        println!("smbc: {:?}", smbc);
                        println!("save: {:?}", save);
                        println!("Error sending debug webhook:\n{}", err);
                    }
                }
                Err(err) => {
                    println!("Error loading state:\n{}", err);
                }
            }
        });
}