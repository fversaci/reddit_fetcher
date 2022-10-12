use roux::util::{FeedOption, TimePeriod};
use roux::{response::BasicThing, submission::SubmissionData, Subreddit};
use strum_macros::{Display, EnumIter, EnumString};

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

pub async fn get_posts(rcmd: RedditCmd) -> Vec<String> {
    let posts = get_posts_raw(rcmd).await;
    let mut ret = Vec::new();
    for post in posts {
        if post.data.stickied {
            continue;
        }
        let piece = format!(
            "{}\n{}",
            post.data.title,
            post.data.url.unwrap_or_default(), // defaults to ""
        );
        ret.push(piece);
    }
    ret
}
