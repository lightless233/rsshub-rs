use std::collections::HashMap;
use std::sync::Arc;
use axum::extract::Query;
use axum::http::HeaderMap;
use rss::{ChannelBuilder, ItemBuilder};
use tokio::sync::Semaphore;

pub async fn pyn3rd(Query(query): Query<HashMap<String, String>>) -> (HeaderMap, String) {

    // 如果有 full 参数，则获取全文
    let full_text = match query.get("full") {
        Some(full) => full == "1",
        None => false,
    };

    // 最终返回用的 rss channel
    let mut channel = ChannelBuilder::default()
        .title("pyn3rd blog")
        .link("https://blog.pyn3rd.com/")
        .description("pyn3rd blog")
        .build();
    let mut headers = HeaderMap::new();
    headers.insert(
        "Content-Type", "application/rss+xml; charset=UTF-8".parse().unwrap(),
    );
    headers.insert("Access-Control-Allow-Origin", "*".parse().unwrap());
    headers.insert("Access-Control-Allow-Methods", "*".parse().unwrap());
    headers.insert("Access-Control-Allow-Credentials", "*".parse().unwrap());
    headers.insert("Access-Control-Allow-Headers", "*".parse().unwrap());

    // 获取首页内容
    let resp = match reqwest::get("https://blog.pyn3rd.com/").await {
        Err(e) => {
            eprintln!("Error while fetch URL: https://blog.pyn3rd.com/ , error: {e:#?}");
            return (headers, channel.to_string());
        }
        Ok(v) => v,
    };
    let html_text = match resp.text().await {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error while fetch index text from URL: https://blog.pyn3rd.com/ , error: {e:#?}");
            return (headers, channel.to_string());
        }
    };

    // 从 html 里解析出文章列表
    let tokio_result = tokio::task::spawn_blocking(move || {
        let mut rss_items = vec![];
        let parsed_html = scraper::Html::parse_document(&html_text);

        let article_item_selector = scraper::Selector::parse(r#"ul[class="post-list"] > li[class="post-item"]"#).unwrap();
        let title_selector = scraper::Selector::parse(r#"a"#).unwrap();
        let time_selector = scraper::Selector::parse(r#"time"#).unwrap();
        let article_items = parsed_html.select(&article_item_selector);

        // <li class="post-item">
        //    <div class="meta">
        //     <time datetime="2023-10-20T09:01:54.000Z" itemprop="datePublished">2023-10-20</time>
        //    </div>
        //    <span>
        //      <a class="" href="/2023/10/20/Java-Deserialization-Vulnerability-Still-Alive/">Java Deserialization Vulnerability Still Alive</a>
        //    </span>
        // </li>
        for item in article_items {
            let first_a_tag = item.select(&title_selector).next();
            if first_a_tag.is_none() {
                eprintln!("Can't fetch item title in URL: https://blog.pyn3rd.com/ , no <a> in li[class=\"post-item\"]");
                continue;
            }
            let a_tag = first_a_tag.unwrap();

            // 解析标题，如果没解析到，使用 URL 填充
            let title = match a_tag.value().attr("title") {
                Some(v) => v.to_string(),
                None => {
                    let t = a_tag.text().next().unwrap().trim();
                    format!("https://blog.pyn3rd.com{t}")
                }
            };

            // 解析 URL，如果没解析到，使用域名填充
            let url = a_tag.value().attr("href").map(|v| format!("https://blog.pyn3rd.com{v}"))
                .unwrap_or("https://blog.pyn3rd.com/".to_string());

            // 解析发布时间
            let time_tag = item.select(&time_selector).next();
            let publish_time = if let Some(time_tag) = time_tag {
                time_tag.value().attr("datetime").unwrap()
            } else {
                ""
            };

            println!("Title: {title}, url: {url}, publish_time: {publish_time}");
            rss_items.push(
                (title, url, publish_time.to_string())
            )
        }

        rss_items
    }).await;
    let rss_items = match tokio_result {
        Ok(v) => v,
        Err(e) => {
            eprint!("Error while fetch each URL in https://blog.pyn3rd.com/ , error: {e:#?}");
            return (headers, channel.to_string());
        }
    };

    // 如果设置了 full 爬取全文的标志，开协程进一步获取文章全文
    let rss_items = if full_text {
        // 初始化并发用的信号量，同时最大并发定为 25
        let limit_semaphore = Arc::new(Semaphore::new(25));

        // 存储所有的协程
        let mut handlers = vec![];

        for item in rss_items {
            let link = &item.1;
            if link == "https://blog.pyn3rd.com/" {
                continue;
            }

            // 获取信号量并启动协程
            limit_semaphore.acquire().await.unwrap().forget();
            handlers.push(tokio::spawn(worker(item, limit_semaphore.clone())));
        }

        // 依次获取每个协程的结果
        let mut rss_items = vec![];
        for handle in handlers {
            match handle.await {
                Ok(res) => {
                    // (title, url, publish_time, full_text)
                    rss_items.push(
                        ItemBuilder::default()
                            .pub_date(res.2)
                            .title(Some(res.0))
                            .link(Some(res.1))
                            .content(res.3)
                            .build()
                    );
                }
                Err(e) => {
                    eprintln!("Error while fetch full content. Error: {e:#?}");
                }
            }
        }

        rss_items
    } else {
        let mut items = vec![];
        for item in rss_items {
            items.push(
                ItemBuilder::default()
                    .pub_date(item.2)
                    .title(Some(item.0))
                    .link(Some(item.1))
                    .build()
            );
        }

        items
    };


    // 返回 RSS 数据
    channel.set_items(rss_items);
    (headers, channel.to_string())
}

/// args: item(title, url, publish_time)
/// return: (title, url, publish_time, full_text)
async fn worker(item: (String, String, String), sem: Arc<Semaphore>) -> (String, String, String, String) {

    let resp = reqwest::get(&item.1).await.unwrap();
    let content = resp.text().await.unwrap();

    let selector = scraper::Selector::parse(r#"article[class="post"]"#).unwrap();
    let parsed_html = scraper::Html::parse_document(&content);
    let article = parsed_html.select(&selector).next().unwrap().html();

    sem.add_permits(1);
    return (item.0, item.1, item.2, article);
}