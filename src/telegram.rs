use crate::persist;
use crate::reddit;
use crate::reddit::{RedReq, RedditCmd};
use crate::{HashSet, MyState, SubredditsCats};
use std::str::FromStr;
use std::sync::Arc;
use strum::IntoEnumIterator;
use teloxide::{
    dispatching::{dialogue, dialogue::InMemStorage, UpdateHandler},
    net::Download,
    payloads,
    prelude::*,
    requests::JsonRequest,
    types::{InlineKeyboardButton, InlineKeyboardMarkup, InputFile, MessageId, ParseMode},
    utils::command::BotCommands,
};
use tokio::fs;
use uuid::Uuid;

type MyDialogue = Dialogue<State, InMemStorage<State>>;
pub type HandlerResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

#[derive(Default, Clone)]
pub enum State {
    #[default]
    Bouncer,
    Start {
        my_state: Arc<MyState>,
    },
    SelectSubreddit {
        my_state: Arc<MyState>,
        prev: Option<MessageId>,
    },
    SelectView {
        my_state: Arc<MyState>,
        rcmd: RedditCmd,
        prev: Option<MessageId>,
    },
    SelectTot {
        my_state: Arc<MyState>,
        rcmd: RedditCmd,
        prev: Option<MessageId>,
    },
    IssueCmd {
        my_state: Arc<MyState>,
        rcmd: RedditCmd,
        prev: Option<MessageId>,
    },
    NextPage {
        my_state: Arc<MyState>,
        rcmd: RedditCmd,
        prev: Option<MessageId>,
    },
    AcceptJSON {
        my_state: Arc<MyState>,
    },
}

#[derive(BotCommands, Clone)]
#[command(
    rename_rule = "lowercase",
    description = "These commands are supported:"
)]
pub enum Command {
    #[command(description = "Show available commands.")]
    Help,
    #[command(description = "(Re)start the menu.")]
    Start,
    #[command(description = "Download JSON list of subreddits, to be edited.")]
    GetSubs,
    #[command(description = "Upload your customized JSON list of subreddits.")]
    SendSubs,
    #[command(description = "Delete your customized JSON list of subreddits.")]
    DelSubs,
}

pub fn schema(
    my_state: Arc<MyState>,
) -> UpdateHandler<Box<dyn std::error::Error + Send + Sync + 'static>> {
    use dptree::case;

    let tmp_state = my_state.clone();
    let run_bouncer = move |bot: Bot, dialogue: MyDialogue, msg: Message| {
        bouncer(bot, dialogue, msg, tmp_state.clone())
    };
    let tmp_state = my_state.clone();
    let run_get_json = move |bot: Bot, msg: Message| get_json(bot, msg, tmp_state.clone());
    let tmp_state = my_state.clone();
    let run_send_json = move |bot: Bot, dialogue: MyDialogue, msg: Message| {
        send_json(bot, dialogue, msg, tmp_state.clone())
    };
    let run_del_json = move |bot: Bot, dialogue: MyDialogue, msg: Message| {
        del_json(bot, dialogue, msg, my_state.clone())
    };

    let command_handler = teloxide::filter_command::<Command, _>()
        .branch(case![Command::Help].endpoint(help))
        .branch(case![Command::GetSubs].endpoint(run_get_json))
        .branch(case![Command::SendSubs].endpoint(run_send_json))
        .branch(case![Command::DelSubs].endpoint(run_del_json))
        .branch(case![Command::Start].endpoint(run_bouncer));

    let message_handler = Update::filter_message()
        .branch(command_handler)
        .branch(case![State::AcceptJSON { my_state }].endpoint(accept_json))
        .branch(case![State::SelectSubreddit { my_state, prev }].endpoint(sub_from_msg))
        .branch(dptree::endpoint(invalid_state));

    let callback_query_handler = Update::filter_callback_query()
        .branch(case![State::SelectSubreddit { my_state, prev }].endpoint(select_subreddit))
        .branch(
            case![State::SelectView {
                my_state,
                rcmd,
                prev
            }]
            .endpoint(select_view),
        )
        .branch(
            case![State::SelectTot {
                my_state,
                rcmd,
                prev
            }]
            .endpoint(select_tot),
        )
        .branch(
            case![State::IssueCmd {
                my_state,
                rcmd,
                prev
            }]
            .endpoint(issue_cmd),
        )
        .branch(
            case![State::NextPage {
                my_state,
                rcmd,
                prev
            }]
            .endpoint(next_page),
        );

    dialogue::enter::<Update, InMemStorage<State>, State, _>()
        .branch(message_handler)
        .branch(callback_query_handler)
}

async fn help(bot: Bot, msg: Message) -> HandlerResult {
    bot.send_message(msg.chat.id, Command::descriptions().to_string())
        .await?;
    Ok(())
}

async fn invalid_state(bot: Bot, msg: Message) -> HandlerResult {
    bot.send_message(
        msg.chat.id,
        "Unable to handle the message. Type /help to see the usage.",
    )
    .await?;
    Ok(())
}

async fn send_json(
    bot: Bot,
    dialogue: MyDialogue,
    msg: Message,
    my_state: Arc<MyState>,
) -> HandlerResult {
    // whitelist check
    let chat_id = msg.chat.id;
    let wl = &my_state.my_conf.id_whitelist;
    if !allowed(&chat_id, wl) {
        bot.send_message(chat_id, "Sorry dude, you're not in the whitelist.")
            .await?;
        return Ok(());
    }
    bot.send_message(
        chat_id,
        "Ok, please send the customized JSON file (as an attachment).",
    )
    .await?;
    dialogue.update(State::AcceptJSON { my_state }).await?;
    Ok(())
}

async fn accept_json(
    bot: Bot,
    dialogue: MyDialogue,
    msg: Message,
    my_state: Arc<MyState>,
) -> HandlerResult {
    let chat_id = msg.chat.id;
    let doc = msg.document();
    let max_size = 20000;
    match doc {
        None => {
            bot.send_message(
                chat_id,
                "JSON file is missing, please send it as an attachment.",
            )
            .await?;
        }
        Some(doc) if doc.file.size < max_size => {
            let file = bot.get_file(&doc.file.id).await?;
            let tmpfile = format!("/tmp/{}.json", Uuid::new_v4());
            {
                // write and close tempfile
                let mut dst = fs::File::create(&tmpfile).await?;
                bot.download_file(&file.path, &mut dst).await?;
            }
            let subs_txt = fs::read_to_string(&tmpfile).await?;
            std::fs::remove_file(tmpfile)?;
            let new_subs = serde_json::from_str::<SubredditsCats>(&subs_txt);
            match new_subs {
                Err(e) => {
                    bot.send_message(
                        chat_id,
                        format!("Error while parsing your JSON file: {}.", e),
                    )
                    .await?;
                }
                Ok(subs) => {
                    persist::insert_pref(&my_state.db, chat_id, &subs)
                        .await
                        .expect("Error: cannot insert values in DB");
                    bot.send_message(chat_id, "Your subreddits have been succesfully updated.")
                        .await?;
                    // restart menu
                    dialogue
                        .update(State::Start {
                            my_state: my_state.clone(),
                        })
                        .await?;
                    select_category(bot, dialogue, my_state).await?;
                }
            }
        }
        _ => {
            bot.send_message(
                chat_id,
                format!(
                    "JSON file is too big, must be smaller than {} bytes. Please send it again.",
                    max_size
                ),
            )
            .await?;
        }
    }
    Ok(())
}

async fn del_json(
    bot: Bot,
    dialogue: MyDialogue,
    msg: Message,
    my_state: Arc<MyState>,
) -> HandlerResult {
    let chat_id = msg.chat.id;
    let num = persist::del_prefs(&my_state.db, chat_id).await?;
    let txt = match num {
        0 => "There's no saved list to delete.",
        _ => "Your customized list of subreddits has been deleted.",
    };
    bot.send_message(chat_id, txt).await?;
    bouncer(bot, dialogue, msg, my_state).await
}

async fn get_json(bot: Bot, msg: Message, my_state: Arc<MyState>) -> HandlerResult {
    // whitelist check
    let chat_id = msg.chat.id;
    let wl = &my_state.my_conf.id_whitelist;
    if !allowed(&chat_id, wl) {
        bot.send_message(chat_id, "Sorry dude, you're not in the whitelist.")
            .await?;
        return Ok(());
    }

    let subs = get_catsubs(&my_state, chat_id).await;
    let mut subs = serde_json::to_string_pretty(&subs).unwrap();
    subs.push('\n'); // add EOL
    bot.send_document(
        chat_id,
        InputFile::memory(subs).file_name("my_subreddits.json"),
    )
    .await?;
    Ok(())
}

fn allowed(chat_id: &ChatId, whitelist: &HashSet<ChatId>) -> bool {
    whitelist.is_empty() | whitelist.contains(chat_id)
}

async fn bouncer(
    bot: Bot,
    dialogue: MyDialogue,
    msg: Message,
    my_state: Arc<MyState>,
) -> HandlerResult {
    bot.set_my_commands(Command::bot_commands()).await?;
    // whitelist check
    let chat_id = msg.chat.id;
    let wl = &my_state.my_conf.id_whitelist;
    if !allowed(&chat_id, wl) {
        bot.send_message(chat_id, "Sorry dude, you're not in the whitelist.")
            .await?;
        return Ok(());
    }
    // set initial state
    dialogue
        .update(State::Start {
            my_state: my_state.clone(),
        })
        .await?;
    select_category(bot, dialogue, my_state).await
}

async fn get_categories(my_state: &MyState, chat_id: ChatId) -> Vec<String> {
    let db_cats = persist::fetch_cats(&my_state.db, chat_id)
        .await
        .expect("Error while querying the DB");
    match db_cats {
        Some(cats) => cats,
        None => {
            let mut cats: Vec<String> = my_state.my_conf.cat_subreddits.keys().cloned().collect();
            cats.sort();
            cats
        }
    }
}

async fn get_catsubs(my_state: &MyState, chat_id: ChatId) -> SubredditsCats {
    let db_subs = persist::fetch_subs(&my_state.db, chat_id)
        .await
        .expect("Error while querying the DB");
    match db_subs {
        Some(subs) => subs,
        None => my_state.my_conf.cat_subreddits.clone(),
    }
}

async fn get_subreddits(my_state: &MyState, category: &String, chat_id: ChatId) -> Vec<String> {
    let def_subs = vec!["All".to_string()];
    let subs = get_catsubs(my_state, chat_id).await;
    subs.get(category).unwrap_or(&def_subs).to_vec()
}

async fn select_category(bot: Bot, dialogue: MyDialogue, my_state: Arc<MyState>) -> HandlerResult {
    let cats_per_row = 3;
    let chat_id = dialogue.chat_id();
    let red_cats = get_categories(&my_state, chat_id).await;
    let red_cats = red_cats.chunks(cats_per_row).map(|r| {
        r.iter()
            .map(|red_cat| InlineKeyboardButton::callback(red_cat.clone(), red_cat.clone()))
    });
    let txt_msg = "Select a category (or type in a subreddit):".to_string();
    let sent = bot
        .send_message(dialogue.chat_id(), txt_msg)
        .reply_markup(InlineKeyboardMarkup::new(red_cats))
        .await?;
    let prev = Some(sent.id);
    dialogue
        .update(State::SelectSubreddit { my_state, prev })
        .await?;
    Ok(())
}

async fn clean_buttons(bot: Bot, chat_id: ChatId, m_id: Option<MessageId>) -> HandlerResult {
    // clean old buttons?
    if let Some(m_id) = m_id {
        bot.edit_message_reply_markup(chat_id, m_id).await?;
    }
    Ok(())
}

async fn select_subreddit(
    bot: Bot,
    dialogue: MyDialogue,
    q: CallbackQuery,
    tup_state: (Arc<MyState>, Option<MessageId>),
) -> HandlerResult {
    let subs_per_row = 3;
    let (my_state, m_id) = tup_state;
    let chat_id = dialogue.chat_id();
    clean_buttons(bot.clone(), chat_id, m_id).await?;
    let category = &q.data.unwrap_or_else(|| "News".to_string());
    let rcmd = RedditCmd {
        view: RedReq::Hot,
        subreddit: "".to_string(),
        tot: 0,
        category: category.to_string(),
        last_seen: None,
    };
    let red_subs = get_subreddits(&my_state, category, chat_id).await;
    let red_subs = red_subs.chunks(subs_per_row).map(|r| {
        r.iter()
            .map(|red_sub| InlineKeyboardButton::callback(red_sub.clone(), red_sub.clone()))
    });
    let txt_msg = format!("Select a subreddit from {}:", category);
    let sent = bot
        .send_message(chat_id, txt_msg)
        .reply_markup(InlineKeyboardMarkup::new(red_subs))
        .await?;
    let prev = Some(sent.id);
    dialogue
        .update(State::SelectView {
            my_state,
            rcmd,
            prev,
        })
        .await?;
    Ok(())
}

async fn sub_from_msg(
    bot: Bot,
    dialogue: MyDialogue,
    msg: Message,
    tup_state: (Arc<MyState>, Option<MessageId>),
) -> HandlerResult {
    let (my_state, m_id) = tup_state;
    // clean_buttons(bot.clone(), chat_id, m_id).await?;
    let sub = msg.text().unwrap_or("All").to_string();
    let rcmd = RedditCmd {
        view: RedReq::Hot,
        subreddit: sub.clone(),
        tot: 0,
        category: "Custom".to_string(),
        last_seen: None,
    };
    select_view_core(bot, dialogue, sub, (my_state, rcmd, m_id)).await
}

async fn select_view(
    bot: Bot,
    dialogue: MyDialogue,
    q: CallbackQuery,
    tup_state: (Arc<MyState>, RedditCmd, Option<MessageId>),
) -> HandlerResult {
    // extract subreddit
    let subreddit = q.data.unwrap_or_else(|| "all".to_string());
    select_view_core(bot, dialogue, subreddit, tup_state).await
}

async fn select_view_core(
    bot: Bot,
    dialogue: MyDialogue,
    subreddit: String,
    tup_state: (Arc<MyState>, RedditCmd, Option<MessageId>),
) -> HandlerResult {
    // save subreddit
    let (my_state, rcmd, m_id) = tup_state;
    let chat_id = dialogue.chat_id();
    clean_buttons(bot.clone(), chat_id, m_id).await?;
    let rcmd = RedditCmd {
        subreddit: subreddit.clone(),
        ..rcmd
    };
    // choose view
    let red_cmds = RedReq::iter()
        .map(|rc| rc.to_string())
        .map(|red_cmd| InlineKeyboardButton::callback(red_cmd.clone(), red_cmd));
    let txt_msg = format!("Choose what to view from {}:", subreddit);
    let sent = bot
        .send_message(chat_id, txt_msg)
        .reply_markup(InlineKeyboardMarkup::new([red_cmds]))
        .await?;
    let prev = Some(sent.id);
    dialogue
        .update(State::SelectTot {
            my_state,
            rcmd,
            prev,
        })
        .await?;
    Ok(())
}

async fn select_tot(
    bot: Bot,
    dialogue: MyDialogue,
    q: CallbackQuery,
    tup_state: (Arc<MyState>, RedditCmd, Option<MessageId>),
) -> HandlerResult {
    let (my_state, rcmd, m_id) = tup_state;
    let chat_id = dialogue.chat_id();
    clean_buttons(bot.clone(), chat_id, m_id).await?;
    // save view
    let view = &q.data.unwrap_or_else(|| "Hot".to_string());
    let view = RedReq::from_str(view).unwrap_or(RedReq::Hot);
    let rcmd = RedditCmd {
        view: view.clone(),
        ..rcmd
    };
    // select tot
    let mut red_tots: Vec<u32> = (1..=3_u32).collect();
    red_tots.extend(vec![5, 7, 10, 20, 40]);
    let red_tots = red_tots
        .iter()
        .map(|rt| rt.to_string())
        .map(|red_tot| InlineKeyboardButton::callback(red_tot.clone(), red_tot));
    let txt_msg = format!("How many {} posts?:", view);
    let sent = bot
        .send_message(chat_id, txt_msg)
        .reply_markup(InlineKeyboardMarkup::new([red_tots]))
        .await?;
    let prev = Some(sent.id);
    dialogue
        .update(State::IssueCmd {
            my_state,
            rcmd,
            prev,
        })
        .await?;
    Ok(())
}

async fn send_page(
    bot: Bot,
    rcmd: &mut RedditCmd,
    chat_id: ChatId,
) -> Result<Option<MessageId>, Box<dyn std::error::Error + Send + Sync>> {
    let summary = format!(
        "*Shown {} {} posts from {} / {}*",
        rcmd.tot, rcmd.view, rcmd.category, rcmd.subreddit
    );
    reddit::send_posts(bot.clone(), chat_id, rcmd).await?;
    let md = payloads::SendMessage::new(chat_id, summary);
    type Sender = JsonRequest<payloads::SendMessage>;
    let sent = Sender::new(bot.clone(), md.clone().parse_mode(ParseMode::MarkdownV2)).await;
    // If markdown cannot be parsed, send it as raw text
    if sent.is_err() {
        Sender::new(bot.clone(), md.clone()).await?;
    };
    // select next page or quit
    let cmd_next = vec![
        ("Done".to_string(), "Done".to_string()),
        ("Show another page".to_string(), "Next".to_string()),
    ];
    let cmd_next = cmd_next
        .iter()
        .map(|cmd| InlineKeyboardButton::callback(cmd.0.to_owned(), cmd.1.to_owned()));
    let sent = bot
        .send_message(chat_id, "What now?")
        .reply_markup(InlineKeyboardMarkup::new([cmd_next]))
        .await?;
    Ok(Some(sent.id))
}

async fn issue_cmd(
    bot: Bot,
    dialogue: MyDialogue,
    q: CallbackQuery,
    tup_state: (Arc<MyState>, RedditCmd, Option<MessageId>),
) -> HandlerResult {
    let (my_state, rcmd, m_id) = tup_state;
    let chat_id = dialogue.chat_id();
    clean_buttons(bot.clone(), chat_id, m_id).await?;
    let tot: u32 = q
        .data
        .unwrap_or_else(|| "1".to_string())
        .parse()
        .unwrap_or(1);
    let mut rcmd = RedditCmd { tot, ..rcmd };
    log::info!("{chat_id} {rcmd:?}");
    // send pages and show next/quit menu
    let prev = send_page(bot.clone(), &mut rcmd, chat_id).await?;
    dialogue
        .update(State::NextPage {
            my_state,
            rcmd,
            prev,
        })
        .await?;
    Ok(())
}

async fn next_page(
    bot: Bot,
    dialogue: MyDialogue,
    q: CallbackQuery,
    tup_state: (Arc<MyState>, RedditCmd, Option<MessageId>),
) -> HandlerResult {
    let (my_state, mut rcmd, m_id) = tup_state;
    let chat_id = dialogue.chat_id();
    clean_buttons(bot.clone(), chat_id, m_id).await?;
    let cmd_next = &q.data.unwrap_or_else(|| "Done".to_string());
    match cmd_next.as_str() {
        "Next" => {
            let prev = send_page(bot, &mut rcmd, chat_id).await?;
            dialogue
                .update(State::NextPage {
                    my_state,
                    rcmd,
                    prev,
                })
                .await?;
            Ok(())
        }
        _ => {
            // "Done"
            dialogue
                .update(State::Start {
                    my_state: my_state.clone(),
                })
                .await?;
            select_category(bot, dialogue, my_state).await
        }
    }
}
