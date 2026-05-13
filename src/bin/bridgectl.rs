use std::{env, process::Command};

use anyhow::{Context, Result, bail};
use reqwest::StatusCode;
use serde_json::{Value, json};

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    let base_url = env::var("BRIDGE_URL").unwrap_or_else(|_| "http://localhost:8080".to_owned());
    let client = reqwest::Client::new();

    match args.first().map(String::as_str) {
        Some("status") | None => status(&client, &base_url).await,
        Some("tokens") => get_json(&client, &format!("{base_url}/v0/tokens")).await,
        Some("flows") => get_json(&client, &format!("{base_url}/demo/flows")).await,
        Some("flow") => {
            let id = args
                .get(1)
                .context("usage: bridgectl flow <correlation-id>")?;
            get_json(&client, &format!("{base_url}/demo/flows/{id}")).await
        }
        Some("quote") => quote(&client, &base_url, &args[1..]).await,
        Some("demo") => demo(&client, &base_url, &args[1..]).await,
        Some("logs") => docker(&["compose", "--env-file", ".env", "logs", "-f", "bridge"]),
        Some("reset") => docker(&[
            "compose",
            "--env-file",
            ".env",
            "down",
            "--volumes",
            "--remove-orphans",
        ]),
        Some("help") | Some("--help") | Some("-h") => {
            print_help();
            Ok(())
        }
        Some(other) => {
            print_help();
            bail!("unknown command: {other}")
        }
    }
}

async fn status(client: &reqwest::Client, base_url: &str) -> Result<()> {
    let health = client
        .get(format!("{base_url}/healthz"))
        .send()
        .await
        .context("health request failed")?;
    let info = client
        .get(format!("{base_url}/demo/info"))
        .send()
        .await
        .context("demo info request failed")?;
    let tokens = client
        .get(format!("{base_url}/v0/tokens"))
        .send()
        .await
        .context("tokens request failed")?;
    let flows = client
        .get(format!("{base_url}/demo/flows"))
        .send()
        .await
        .context("flows request failed")?;

    print_json(json!({
        "bridgeUrl": base_url,
        "healthz": health.status().as_u16(),
        "demo": response_json_or_status(info).await?,
        "tokens": response_json_or_status(tokens).await?,
        "flows": response_json_or_status(flows).await?,
    }))
}

async fn quote(client: &reqwest::Client, base_url: &str, args: &[String]) -> Result<()> {
    let direction = args.first().map(String::as_str).unwrap_or("inbound");
    let asset = flag(args, "--asset").unwrap_or("eth");
    let amount = flag(args, "--amount").unwrap_or("1000000000000");
    let evm_prefix = evm_asset_prefix(client, base_url).await?;
    let recipient = flag(args, "--recipient").unwrap_or(match direction {
        "outbound" => "0x9965507D1a55bcC2695C58ba16FB37d819B0A4dc",
        _ => "miden-recipient-address",
    });
    let refund_to = flag(args, "--refund-to").unwrap_or(match direction {
        "outbound" => "miden-refund-address",
        _ => "0x9965507D1a55bcC2695C58ba16FB37d819B0A4dc",
    });
    let (origin_asset, destination_asset) = match direction {
        "inbound" => (
            format!("{evm_prefix}:{asset}"),
            format!("miden-testnet:{asset}"),
        ),
        "outbound" => (
            format!("miden-testnet:{asset}"),
            format!("{evm_prefix}:{asset}"),
        ),
        other => bail!("direction must be inbound or outbound, got {other}"),
    };
    let payload = json!({
        "dry": false,
        "depositMode": "SIMPLE",
        "swapType": "EXACT_INPUT",
        "slippageTolerance": 100.0,
        "originAsset": origin_asset,
        "depositType": "ORIGIN_CHAIN",
        "destinationAsset": destination_asset,
        "amount": amount,
        "refundTo": refund_to,
        "refundType": "ORIGIN_CHAIN",
        "recipient": recipient,
        "recipientType": "DESTINATION_CHAIN",
        "deadline": "2027-01-01T00:00:00Z"
    });
    post_json(client, &format!("{base_url}/v0/quote"), payload).await
}

async fn evm_asset_prefix(client: &reqwest::Client, base_url: &str) -> Result<String> {
    let response = client
        .get(format!("{base_url}/demo/info"))
        .send()
        .await
        .context("demo info request failed")?;
    if response.status().is_success() {
        let value = response.json::<Value>().await?;
        if let Some(profile) = value.get("runtimeProfile").and_then(Value::as_str) {
            return profile_to_evm_asset_prefix(profile);
        }
    }

    match env::var("BRIDGE_PROFILE") {
        Ok(profile) => profile_to_evm_asset_prefix(&profile),
        Err(_) => Ok("eth-anvil".to_owned()),
    }
}

fn profile_to_evm_asset_prefix(profile: &str) -> Result<String> {
    match profile {
        "anvil" => Ok("eth-anvil".to_owned()),
        "sepolia" => Ok("eth-sepolia".to_owned()),
        other => bail!("unsupported BRIDGE_PROFILE {other}"),
    }
}

async fn demo(client: &reqwest::Client, base_url: &str, args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("inbound") => {
            post_json(
                client,
                &format!("{base_url}/demo/flows/inbound/start"),
                demo_payload(args),
            )
            .await
        }
        Some("claim") => {
            let account_id = args
                .get(1)
                .context("usage: bridgectl demo claim <account-id>")?;
            post_json(
                client,
                &format!("{base_url}/demo/flows/inbound/claim"),
                json!({ "accountId": account_id }),
            )
            .await
        }
        Some("outbound-fund") => {
            post_json(
                client,
                &format!("{base_url}/demo/flows/outbound/fund"),
                demo_payload(args),
            )
            .await
        }
        Some("outbound-submit") => {
            let account_id = args
                .get(1)
                .context("usage: bridgectl demo outbound-submit <sender-account-id>")?;
            post_json(
                client,
                &format!("{base_url}/demo/flows/outbound/submit"),
                json!({
                    "senderAccountId": account_id,
                    "asset": flag(args, "--asset").unwrap_or("eth"),
                    "amount": flag(args, "--amount").unwrap_or("1000000000000")
                }),
            )
            .await
        }
        _ => {
            print_help();
            bail!("usage: bridgectl demo inbound|claim|outbound-fund|outbound-submit")
        }
    }
}

fn demo_payload(args: &[String]) -> Value {
    json!({
        "asset": flag(args, "--asset").unwrap_or("eth"),
        "amount": flag(args, "--amount").unwrap_or("1000000000000")
    })
}

async fn get_json(client: &reqwest::Client, url: &str) -> Result<()> {
    let response = client.get(url).send().await?;
    ensure_ok(response.status(), url)?;
    print_json(response.json::<Value>().await?)
}

async fn post_json(client: &reqwest::Client, url: &str, payload: Value) -> Result<()> {
    let response = client.post(url).json(&payload).send().await?;
    ensure_ok(response.status(), url)?;
    print_json(response.json::<Value>().await?)
}

async fn response_json_or_status(response: reqwest::Response) -> Result<Value> {
    let status = response.status();
    if status == StatusCode::OK {
        Ok(response.json::<Value>().await?)
    } else {
        Ok(json!({ "status": status.as_u16() }))
    }
}

fn ensure_ok(status: StatusCode, url: &str) -> Result<()> {
    if status.is_success() {
        Ok(())
    } else {
        bail!("{url} returned HTTP {status}")
    }
}

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].as_str())
}

fn print_json(value: Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

fn docker(args: &[&str]) -> Result<()> {
    let status = Command::new("docker").args(args).status()?;
    if status.success() {
        Ok(())
    } else {
        bail!("docker {} exited {status}", args.join(" "))
    }
}

fn print_help() {
    eprintln!(
        "bridgectl commands:
  status
  tokens
  quote inbound|outbound [--asset eth] [--amount 1000000000000] [--recipient ...] [--refund-to ...]
  demo inbound [--asset eth] [--amount 1000000000000]
  demo claim <miden-account-id>
  demo outbound-fund [--asset eth] [--amount 1000000000000]
  demo outbound-submit <miden-account-id> [--asset eth] [--amount 1000000000000]
  flows
  flow <correlation-id>
  logs
  reset"
    );
}
