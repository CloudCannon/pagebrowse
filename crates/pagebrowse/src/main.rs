use futures::future::join_all;
use pagebrowse_lib::*;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() -> Result<(), PagebrowseError> {
    // TODO: Make parallel
    // let browser = PagebrowseBuilder::new(20).visible(true).build()?;
    // let windows = join_all((0..20).map(|_| browser.get_window()).collect::<Vec<_>>()).await;

    let browsers = (0..20)
        .map(|_| PagebrowseBuilder::new(1).visible(true).build())
        .collect::<Vec<_>>();

    let windows = join_all(
        browsers
            .iter()
            .flatten()
            .map(|browser| browser.get_window()),
    )
    .await;

    join_all(
        windows
            .iter()
            .flatten()
            .map(|window| window.navigate("https://cloudcannon.com/".into())),
    )
    .await;

    join_all(
        windows
            .iter()
            .flatten()
            .map(|window| window.resize_window(1920, 1080)),
    )
    .await;

    sleep(Duration::from_secs(2)).await;

    join_all(windows.iter().flatten().enumerate().map(|(i, window)| {
        window.evaluate_script(format!(
            "document.querySelector(`h1`).innerText = `Window {i}`;"
        ))
    }))
    .await;

    // window
    //     .evaluate_script("document.querySelector(`h1`).innerText = `🦀 🦀 🦀 🦀`;".into())
    //     .await
    //     .unwrap();
    // sleep(Duration::from_millis(100)).await;

    join_all(
        windows
            .iter()
            .flatten()
            .enumerate()
            .map(|(i, window)| window.screenshot(format!("screenshot-{i}.webp").into())),
    )
    .await;

    println!("Done?");

    loop {}
}
