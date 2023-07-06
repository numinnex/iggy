mod common;
use crate::common::http::HttpClientFactory;
use crate::common::scenarios::system_scenario;

#[tokio::test]
async fn system_scenario_should_be_valid() {
    let client_factory = HttpClientFactory {};
    system_scenario::run(&client_factory).await;
}
