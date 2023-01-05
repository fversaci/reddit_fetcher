use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::sync::Arc;
use teloxide::{dispatching::dialogue::InMemStorage, prelude::*};

mod persist;
mod reddit;
mod telegram;

pub type SubredditsCats = HashMap<String, Vec<String>>;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StartEnd {
    starts: Vec<String>,
    endings: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UrlMatches {
    image: StartEnd,
    video: StartEnd,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MyBotConfig {
    cat_subreddits: SubredditsCats,
    id_whitelist: HashSet<ChatId>,
    url_matches: UrlMatches,
}

#[derive(Clone, Debug)]
pub struct MyState {
    my_conf: MyBotConfig,
    db: SqlitePool,
}

fn get_conf() -> MyBotConfig {
    let fname = "conf/defaults.json";
    let conf_txt = fs::read_to_string(fname)
        .unwrap_or_else(|_| panic!("Cannot find configuration file: {}", fname));
    let my_conf: MyBotConfig = serde_json::from_str(&conf_txt)
        .unwrap_or_else(|err| panic!("Unable to parse configuration file {}: {}", fname, err));
    my_conf
}

#[tokio::main]
async fn main() {
    pretty_env_logger::init();
    log::info!("Starting bot...");
    let bot = Bot::from_env();
    let my_conf = get_conf();
    log::debug!("{my_conf:?}");
    let db = persist::open_db().await.expect("Cannot open DB");
    let my_state = Arc::new(MyState { my_conf, db });
    Dispatcher::builder(bot, telegram::schema(my_state))
        .dependencies(dptree::deps![InMemStorage::<telegram::State>::new()])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;
}
