use crate::UrlMatches;
use anyhow::Result;
use roux::util::{FeedOption, TimePeriod};
use roux::{response::BasicThing, submission::SubmissionData, Subreddit};
use std::fs;
use strum_macros::{Display, EnumIter, EnumString};
use teloxide::payloads::{SendDocumentSetters, SendPhotoSetters, SendVideoSetters};
use teloxide::prelude::{ChatId, Requester};
use teloxide::types::{InputFile, Message};
use teloxide::Bot;
use tokio::process::Command;
use url::Url;
use uuid::Uuid;

#[derive(Display, Debug, Clone, EnumIter, EnumString)]
pub enum RedReq {
    Rise,
    Hot,
    TopD,
    TopW,
    TopM,
    TopY,
    TopA,
}

#[derive(Clone, Debug)]
pub struct RedditCmd {
    pub view: RedReq,
    pub subreddit: String,
    pub tot: u32,
    pub category: String,
    pub last_seen: Option<String>,
}

#[derive(Debug)]
enum FSFile {
    Image { f: String },
    Video { f: String },
}

impl FSFile {
    fn get_f(&self) -> String {
        match self {
            FSFile::Image { f } => f.to_string(),
            FSFile::Video { f } => f.to_string(),
        }
    }
    async fn send_out(
        &self,
        bot: &Bot,
        chat_id: ChatId,
        fname: InputFile,
        tit: &String,
    ) -> Result<Message, teloxide::RequestError> {
        match self {
            FSFile::Image { f: _ } => {
                // send as image
                let res = bot.send_photo(chat_id, fname.clone()).caption(tit).await;
                if res.is_ok() {
                    res
                }
                // if resolution is too high, send as document
                else {
                    bot.send_document(chat_id, fname).caption(tit).await
                }
            }
            FSFile::Video { f: _ } => bot.send_video(chat_id, fname).caption(tit).await,
        }
    }
}

async fn get_posts_raw(rcmd: &mut RedditCmd) -> Vec<BasicThing<SubmissionData>> {
    let mut subreddit = rcmd.subreddit.clone();
    subreddit.retain(|c| !c.is_whitespace()); // remove whitespaces
    let subreddit = Subreddit::new(&subreddit);
    let tot = rcmd.tot;
    let mut fopts = FeedOption::new().limit(tot);
    if let Some(aft) = &rcmd.last_seen {
        fopts = fopts.after(aft); // seeing next page
    }
    let view = match &rcmd.view {
        RedReq::Hot => subreddit.hot(tot, Some(fopts)).await,
        RedReq::Rise => subreddit.rising(tot, Some(fopts)).await,
        rr => {
            // Variants of Top command
            fopts = match rr {
                RedReq::TopD => fopts.period(TimePeriod::Today),
                RedReq::TopW => fopts.period(TimePeriod::ThisWeek),
                RedReq::TopM => fopts.period(TimePeriod::ThisMonth),
                RedReq::TopY => fopts.period(TimePeriod::ThisYear),
                RedReq::TopA => fopts.period(TimePeriod::AllTime),
                _ => fopts, // unreachable
            };
            subreddit.top(tot, Some(fopts)).await
        }
    };
    match view {
        Ok(stuff) => {
            rcmd.last_seen = stuff.data.after;
            stuff.data.children
        }
        Err(_) => Vec::new(),
    }
}

pub async fn send_post(
    post: BasicThing<SubmissionData>,
    bot: Bot,
    chat_id: ChatId,
    url_matches: &UrlMatches,
) -> Result<Message, teloxide::RequestError> {
    let max_mb = 50; // 50 MiB
    let max_size = max_mb * 1_048_576;
    let tit = post.data.title;
    let url = post.data.url.unwrap_or_default(); // defaults to ""
    let alt_msg = format!("{}\n{}", &tit, &url);
    if url.is_empty() {
        bot.send_message(chat_id, &tit).await
    } else {
        let mut res;
        let tmpfile = download(&url, max_mb, url_matches).await?;
        if let Some(tmpfile) = tmpfile {
            let f = tmpfile.get_f();
            let sz = fs::metadata(&f)?.len();
            if sz > max_size {
                log::info!("File too big to be sent, sending URL instead.");
                res = bot.send_message(chat_id, alt_msg).await;
            } else {
                let fname = InputFile::file(&f);
                res = tmpfile.send_out(&bot, chat_id, fname, &tit).await;
                if res.is_err() {
                    log::info!("Cannot send file: {}", res.unwrap_err());
                    res = bot.send_message(chat_id, alt_msg).await;
                }
            }
            std::fs::remove_file(f)?;
            res
        } else {
            bot.send_message(chat_id, alt_msg).await
        }
    }
}

pub async fn send_posts(
    bot: Bot,
    chat_id: ChatId,
    rcmd: &mut RedditCmd,
    url_matches: &UrlMatches,
) -> Result<()> {
    let p_raw = get_posts_raw(rcmd).await;
    let mut posts_sent = Vec::new();
    for post in p_raw {
        if post.data.stickied {
            continue;
        }
        let sent = send_post(post, bot.clone(), chat_id, url_matches);
        posts_sent.push(sent);
    }
    for sent in posts_sent {
        sent.await?;
    }
    Ok(())
}

fn get_type(url: &str, url_matches: &UrlMatches) -> Option<FSFile> {
    let mut is_video = false;
    for s in &url_matches.video.endings {
        is_video |= url.ends_with(s);
    }
    for s in &url_matches.video.starts {
        is_video |= url.starts_with(s);
    }
    if is_video {
        return Some(FSFile::Video { f: "".to_string() });
    }
    let mut is_image = false;
    for s in &url_matches.image.endings {
        is_image |= url.ends_with(s);
    }
    for s in &url_matches.image.starts {
        is_image |= url.starts_with(s);
    }
    if is_image {
        return Some(FSFile::Image { f: "".to_string() });
    }
    None
}

async fn download(
    url: &str,
    max_mb: u64,
    url_matches: &UrlMatches,
) -> Result<Option<FSFile>, teloxide::RequestError> {
    let check = Url::parse(url);
    let max_sz = format!("{}M", max_mb);
    // allow only proper https urls
    if check.is_err() || check.unwrap().scheme() != "https" {
        return Ok(None);
    }
    // command arguments
    let downloader;
    let save_as;
    let ext;
    let typ = get_type(url, url_matches);
    if typ.is_none() {
        return Ok(None);
    }
    let mut args = Vec::new();
    let typ = typ.unwrap();
    // image or video?
    match typ {
        FSFile::Image { f: _ } => {
            downloader = "wget";
            save_as = "-O";
            ext = ".jpg";
        }
        FSFile::Video { f: _ } => {
            downloader = "yt-dlp";
            save_as = "-o";
            ext = ".mp4";
            args.push("--max-filesize");
            args.push(&max_sz);
        }
    }
    let base_dir = "/tmp/red_fetch/";
    fs::create_dir_all(base_dir)?;
    let tmpfile = format!("{}{}{}", base_dir, Uuid::new_v4(), ext);
    let mut new_args = vec!["-q", save_as, &tmpfile, url];
    args.append(&mut new_args);
    let child = Command::new(downloader).args(args).spawn();
    if child.is_err() {
        return Ok(None);
    };

    // Await until the command completes
    let status = child.unwrap().wait().await?;
    let md = fs::metadata(&tmpfile);
    if status.success() && md.is_ok() {
        match typ {
            FSFile::Image { f: _ } => Ok(Some(FSFile::Image { f: tmpfile })),
            FSFile::Video { f: _ } => Ok(Some(FSFile::Video { f: tmpfile })),
        }
    } else {
        Ok(None)
    }
}
