use futures::future::join_all;
use pagebrowse_lib::*;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() {
    let browser = PagebrowseBuilder::new(1)
        .visible(true)
        .build()
        .expect("Should be able to build a browser");

    let window = browser
        .get_window()
        .await
        .expect("Should be able to open a window eventually");

    window
        .navigate("https://cloudcannon.com/pricing".into())
        .await
        .unwrap();

    window.resize_window(1500, 1000).await.unwrap();

    sleep(Duration::from_secs(2)).await;

    window
        .evaluate_script("document.querySelector(`h1`).innerText = `🦀 🦀 🦀 🦀`;".into())
        .await
        .unwrap();
    sleep(Duration::from_millis(100)).await;

    window.screenshot("wowza.webp".into()).await.unwrap();

    println!("Done?");

    loop {}

    // join_all(vec![
    //     launch(PagebrowseOptions {
    //         start_url: "https://cloudcannon.com/".into(),
    //         window_visible: true,
    //     }),
    //     launch(PagebrowseOptions {
    //         start_url: "https://pagefind.app/".into(),
    //         window_visible: true,
    //     }),
    // ])
    // .await;
}

// async fn launch(opts: PagebrowseOptions) {
//     let mut window = PagebrowseApp::new(opts);

//     while let Some(msg) = window.get_response().await {
//         println!("Received {msg:#?}");
//     }
// }
