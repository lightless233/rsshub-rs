use std::collections::HashMap;
use axum::extract::Query;
use axum::http::HeaderMap;
use rss::{ChannelBuilder, ItemBuilder};
use scraper::Html;
use scraper::Selector;


lazy_static::lazy_static! {
    static ref ARTICLE_LIST_SELECTOR: Selector =
        Selector::parse(r#"ul[id="article-list"] > li[class="list-item"]"#).unwrap();
    static ref TITLE_SELECTOR: Selector = Selector::parse(r#"a"#).unwrap();
    static ref ARTICLE_SELECTOR: Selector = Selector::parse(r#"article[class="article"]"#).unwrap();
}

#[axum::debug_handler]
pub async fn secrss(Query(query): Query<HashMap<String, String>>) -> (HeaderMap, String) {
    // 从参数中判断是否需要获取全文
    let full_text = match query.get("full") {
        Some(full) => full == "1",
        None => false,
    };

    // 获取文章列表
    let resp = reqwest::get("https://www.secrss.com/")
        .await
        .expect("request failed.");
    let text = resp.text().await.expect("get text failed.");

    let rss_items = tokio::task::spawn_blocking(move || {
        let html = Html::parse_document(&text);
        let article_items = html.select(&ARTICLE_LIST_SELECTOR);

        let mut rss_items = vec![];
        for item in article_items {
            let first_a = item.select(&TITLE_SELECTOR).next().unwrap();
            let title = match first_a.value().attr("title") {
                Some(title) => title,
                None => first_a.text().next().unwrap().trim(),
            };

            let url = first_a.value().attr("href").unwrap();
            println!("title: {title}, url: {url}");

            // 再进一步把文章内容爬出来
            let article_content = if full_text {
                let article_resp = reqwest::blocking::get(url).expect("request article failed.");
                let article_content = article_resp.text().expect("get article text failed.");
                let article_html = Html::parse_document(&article_content);
                let article = article_html
                    .select(&ARTICLE_SELECTOR)
                    .next()
                    .unwrap()
                    .html();
                Some(article)
            } else {
                None
            };

            rss_items.push(
                ItemBuilder::default()
                    .title(Some(title.to_string()))
                    .link(Some(url.to_string()))
                    .content(article_content)
                    .build(),
            );
        }

        rss_items
    })
        .await
        .expect("spawn_blocking failed.");

    // 构建 RSS
    let mut channel = ChannelBuilder::default()
        .title("安全内参 - 最新资讯".to_string())
        .link("https://www.secrss.com/".to_string())
        .description("安全内参 - 最新资讯".to_string())
        .build();
    channel.set_items(rss_items);

    // 设置返回头
    let mut headers = HeaderMap::new();
    headers.insert(
        "Content-Type",
        "application/rss+xml; charset=UTF-8".parse().unwrap(),
    );
    headers.insert("Access-Control-Allow-Origin", "*".parse().unwrap());
    headers.insert("Access-Control-Allow-Methods", "*".parse().unwrap());
    headers.insert("Access-Control-Allow-Credentials", "*".parse().unwrap());
    headers.insert("Access-Control-Allow-Headers", "*".parse().unwrap());

    (headers, channel.to_string())
}