use roux::{response::BasicThing, submission::SubmissionData, Subreddit};
use teloxide::{prelude::*, utils::command::BotCommands};
use tokio;

async fn get_posts(subred: &str, tot: u32) -> Vec<BasicThing<SubmissionData>> {
    let subreddit = Subreddit::new(subred);
    let hot = subreddit.hot(tot, None).await;
    let posts = hot.unwrap().data.children;
    return posts;
}

async fn get_hot(subred: &str, tot: u32) -> Vec<String> {
    let posts = get_posts(&subred, tot).await;
    let mut ret = Vec::new();
    for post in posts {
        if post.data.stickied || post.data.is_self {
            continue;
        }
        let piece = format!("{}\n--> {}", post.data.title, post.data.url.unwrap());
        ret.push(piece);
    }
    return ret;
}

#[tokio::main]
async fn main() {
    pretty_env_logger::init();
    log::info!("Starting throw dice bot...");

    let bot = Bot::from_env();

    teloxide::commands_repl(bot, answer, Command::ty()).await;
}

#[derive(BotCommands, Clone)]
#[command(
    rename_rule = "lowercase",
    description = "These commands are supported:"
)]
enum Command {
    #[command(description = "display this text.")]
    Help,
    #[command(description = "Get top 3 hot posts from subreddit.")]
    Hot(String),
    #[command(
        description = "get top n hot posts from subreddit.",
        parse_with = "split"
    )]
    HotN { subred: String, tot: u32 },
}

async fn answer(bot: Bot, msg: Message, cmd: Command) -> ResponseResult<()> {
    match cmd {
        Command::Help => {
            bot.send_message(msg.chat.id, Command::descriptions().to_string())
                .await?
        }
        Command::Hot(subred) => {
            let tot = 3;
            let posts = get_hot(&subred, tot).await;
            let welcome = format!("Last {tot} hot pics from {subred}...");
            let mut r = bot.send_message(msg.chat.id, welcome).await?;
            for post in posts {
                r = bot.send_message(msg.chat.id, post).await?;
            }
            r
        }
        Command::HotN { subred, tot } => {
            let posts = get_hot(&subred, tot).await;
            let welcome = format!("Last {tot} hot pics from {subred}...");
            let mut r = bot.send_message(msg.chat.id, welcome).await?;
            for post in posts {
                r = bot.send_message(msg.chat.id, post).await?;
            }
            r
        }
    };
    Ok(())
}
