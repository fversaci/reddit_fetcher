# Reddit-Fetcher Telegram Bot

## Overview

This bot, written in Rust, shows the hot/top entries of selected
[subreddits](https://www.reddit.com/). The subreddits can be chosen
from a user-customizable list, or typed-in directly (whitespaces are
ignored). Per-user preferences are saved in a simple sqlite3 DB.

## Requirements

In case they are installed, Reddit-Fetcher tries and download images
and videos using, respectively,
[wget](https://www.gnu.org/software/wget/) and
[yt-dlp](https://github.com/yt-dlp/yt-dlp) (thus increasing the use
of bandwidth of the bot...)

## Running the bot

Assuming you have cargo correctly setup, just run:

```bash
TELOXIDE_TOKEN=123_YOUR_TELEGRAM_BOT_TOKEN_567 \
DATABASE_URL="sqlite://conf/users.db3" \
RUST_LOG=reddit_fetcher=info \
cargo run --release
```

[![Video example](http://img.youtube.com/vi/yx1IliqIO6s/0.jpg)](http://www.youtube.com/watch?v=yx1IliqIO6s)

## Configuration

### Filtering the user access

The configuration file [conf/defaults.json](conf/defaults.json)
contains an `id_whitelist` field, which can be filled with a list of
allowed Telegram user_ids:

```json
  "id_whitelist": [
    123456789,
    987654321
  ]
```

If the list is left empty, filtering is not performed (i.e., all users
will be able to use the bot).

### Choosing the default subreddits

In the same [configuration file](conf/defaults.json) the field
`cat_subreddits` describes the default categories and subreddits.

### Per-user configuration

Users can download a JSON description of their currently active
subreddits via the `/getsubs` command, upload a customized version via
`/sendsubs` and delete any existing customization with `/delsubs`.


## Author

Reddit-Fetcher Telegram Bot is developed by
  * Francesco Versaci <francesco.versaci@gmail.com>

## License

Reddit-Fetcher Telegram Bot is licensed under the GPLv3 License.  See LICENSE
for further details.
