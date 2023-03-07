use crate::SubredditsCats;
use sqlx::migrate::MigrateDatabase;
use sqlx::Row;
use sqlx::SqlitePool;
use std::env;
use teloxide::prelude::ChatId;

pub async fn open_db() -> Result<SqlitePool, sqlx::Error> {
    let db_url = &env::var("DATABASE_URL").expect("Please set DATABASE_URL environment variable");
    sqlx::Sqlite::create_database(db_url)
        .await
        .expect("Cannot create DB");
    let db = SqlitePool::connect(db_url).await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS user_pref (
            user_id    INTEGER PRIMARY KEY,
            categories TEXT NOT NULL,
            subreddits TEXT NOT NULL
         )",
    )
    .execute(&db)
    .await?;
    Ok(db)
}

pub async fn insert_pref(
    db: &SqlitePool,
    chat_id: ChatId,
    subreds: &SubredditsCats,
) -> Result<(), sqlx::Error> {
    let mut cats: Vec<String> = subreds.keys().cloned().collect();
    cats.sort();
    let mut conn = db.acquire().await?;
    let cats = serde_json::to_string(&cats).unwrap();
    let subs = serde_json::to_string(&subreds).unwrap();
    sqlx::query(
        "INSERT INTO user_pref (user_id, categories, subreddits)
         VALUES (?1, ?2, ?3) ON CONFLICT DO UPDATE SET
         categories=excluded.categories, subreddits=excluded.subreddits",
    )
    .bind(chat_id.0)
    .bind(cats)
    .bind(subs)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub async fn del_prefs(db: &SqlitePool, chat_id: ChatId) -> Result<u64, sqlx::Error> {
    let mut conn = db.acquire().await?;
    let res = sqlx::query("DELETE FROM user_pref WHERE user_id = ?;")
        .bind(chat_id.0)
        .execute(&mut *conn)
        .await?;
    Ok(res.rows_affected())
}

pub async fn fetch_cats(
    db: &SqlitePool,
    chat_id: ChatId,
) -> Result<Option<Vec<String>>, sqlx::Error> {
    let res = sqlx::query("SELECT categories FROM user_pref WHERE user_id = ?;")
        .bind(chat_id.0)
        .fetch_optional(db)
        .await?;
    let res: Option<Vec<String>> = match res {
        Some(r) => {
            let r: String = r.get(0);
            let r: Vec<String> = serde_json::from_str(&r)
                .unwrap_or_else(|_| panic!("Error while parsing DB categories: {}", r));
            Some(r)
        }
        None => None,
    };
    Ok(res)
}

pub async fn fetch_subs(
    db: &SqlitePool,
    chat_id: ChatId,
) -> Result<Option<SubredditsCats>, sqlx::Error> {
    let res = sqlx::query("SELECT subreddits FROM user_pref WHERE user_id = ?;")
        .bind(chat_id.0)
        .fetch_optional(db)
        .await?;
    let res: Option<SubredditsCats> = match res {
        Some(r) => {
            let r: String = r.get(0);
            let r: SubredditsCats = serde_json::from_str(&r)
                .unwrap_or_else(|_| panic!("Error while parsing DB categories: {}", r));
            Some(r)
        }
        None => None,
    };
    Ok(res)
}
