use crate::telegram::HandlerResult;
use anyhow::Result;
use roux::util::{FeedOption, TimePeriod};
use roux::{response::BasicThing, submission::SubmissionData, Subreddit};
use std::fs;
use strum_macros::{Display, EnumIter, EnumString};
use teloxide::prelude::{ChatId, Requester};
use teloxide::types::InputFile;
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
}

#[derive(Debug)]
enum FSFile {
    Image(String),
    Video(String),
}

async fn get_posts_raw(rcmd: RedditCmd) -> Vec<BasicThing<SubmissionData>> {
    let subreddit = Subreddit::new(&rcmd.subreddit);
    let tot = rcmd.tot;
    let view = match rcmd.view {
        RedReq::Hot => subreddit.hot(tot, None).await,
        RedReq::Rise => {
            let options = FeedOption::new().limit(tot);
            subreddit.rising(tot, Some(options)).await
        }
        rr => {
            // Variants of Top command
            let options = match rr {
                RedReq::TopD => FeedOption::new().period(TimePeriod::Today).limit(tot),
                RedReq::TopW => FeedOption::new().period(TimePeriod::ThisWeek).limit(tot),
                RedReq::TopM => FeedOption::new().period(TimePeriod::ThisMonth).limit(tot),
                RedReq::TopY => FeedOption::new().period(TimePeriod::ThisYear).limit(tot),
                RedReq::TopA => FeedOption::new().period(TimePeriod::AllTime).limit(tot),
                _ => FeedOption::new(), // unreachable
            };
            subreddit.top(tot, Some(options)).await
        }
    };
    match view {
        Ok(stuff) => stuff.data.children,
        Err(_) => Vec::new(),
    }
}

pub async fn send_tit_url(bot: &Bot, chat_id: ChatId, tit: String, url: String) -> HandlerResult {
    let tit_url = format!("{}\n{}", tit, url);
    bot.send_message(chat_id, tit_url).await?;
    Ok(())
}

pub async fn send_posts(bot: Bot, chat_id: ChatId, rcmd: RedditCmd) -> HandlerResult {
    let p_raw = get_posts_raw(rcmd).await;
    for post in p_raw {
        if post.data.stickied {
            continue;
        }
        let max_size = 50000000; // 50MB
        let tit = post.data.title;
        let url = post.data.url.unwrap_or_default(); // defaults to ""
        if !url.is_empty() {
            let tmpfile = download(&url).await?;
            if let Some(tmpfile) = tmpfile {
                match tmpfile {
                    FSFile::Image(f) => {
                        let sz = fs::metadata(&f)?.len();
                        if sz > max_size {
                            send_tit_url(&bot, chat_id, tit, url).await?;
                        } else {
                            bot.send_message(chat_id, &tit).await?;
                            let fname = InputFile::file(&f);
                            let res = bot.send_photo(chat_id, fname).await;
                            if res.is_err() {
                                bot.send_message(chat_id, url).await?;
                            }
                        }
                        std::fs::remove_file(f)?;
                    }
                    FSFile::Video(f) => {
                        let sz = fs::metadata(&f)?.len();
                        if sz > max_size {
                            send_tit_url(&bot, chat_id, tit, url).await?;
                        } else {
                            bot.send_message(chat_id, &tit).await?;
                            let fname = InputFile::file(&f);
                            let res = bot.send_video(chat_id, fname).await;
                            if res.is_err() {
                                bot.send_message(chat_id, url).await?;
                            }
                        }
                        std::fs::remove_file(f)?;
                    }
                }
            } else {
                send_tit_url(&bot, chat_id, tit, url).await?;
            }
        } else {
            send_tit_url(&bot, chat_id, tit, url).await?;
        }
    }
    Ok(())
}

fn get_type(url: &str) -> Option<FSFile> {
    if url.ends_with(".mp4")
        | url.ends_with(".mkv")
        | url.ends_with(".webm")
        | url.ends_with(".gifv")
        | url.starts_with("https://v.redd.it")
        | url.starts_with("https://gfycat.com")
    {
        return Some(FSFile::Video("".to_string()));
    }
    if url.ends_with(".jpg")
        | url.ends_with(".jpeg")
        | url.ends_with(".png")
        | url.ends_with(".webp")
        | url.ends_with(".gif")
        | url.starts_with("https://i.redd.it")
        | url.starts_with("https://i.imgur.com")
    {
        return Some(FSFile::Image("".to_string()));
    }
    None
}

async fn download(url: &str) -> Result<Option<FSFile>> {
    let check = Url::parse(url)?;
    // allow only proper https urls
    if check.scheme() != "https" {
        return Ok(None);
    }
    let mut downloader = "yt-dlp";
    let mut save_as = "-o";    
    let typ = get_type(url);
    if typ.is_none() {
        return Ok(None);
    }
    let typ = typ.unwrap();
    let ext = match typ {
        FSFile::Image(_) => {
            // use wget for images
            downloader = "wget";
            save_as = "-O";
            ".jpg"
        }
        FSFile::Video(_) => ".mp4",
    };
    let base_dir = "/tmp/red_fetch/";
    fs::create_dir_all(base_dir)?;
    let tmpfile = format!("{}{}{}", base_dir, Uuid::new_v4(), ext);
    let child = Command::new(downloader)
        .arg("-q")
        .arg(save_as)
        .arg(&tmpfile)
        .arg(url)
        .spawn();
    if child.is_err() {
        return Ok(None);
    };

    // Await until the command completes
    let status = child.unwrap().wait().await?;
    if status.success() {
        match typ {
            FSFile::Image(_) => Ok(Some(FSFile::Image(tmpfile))),
            FSFile::Video(_) => Ok(Some(FSFile::Video(tmpfile))),
        }
    } else {
        Ok(None)
    }
}
