use anyhow::Context;
use bluer::Uuid;
use bluer::gatt::local::{
    Application, Characteristic, CharacteristicRead, CharacteristicReadRequest, Service,
};
use clap::Parser;
use futures::FutureExt;
use log::info;
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Parser, Debug)]
#[command(name = "mqttooth", about = "Bridge MQTT temperature data to BLE")]
struct Args {
    /// MQTT broker host
    #[arg(long, default_value = "localhost")]
    mqtt_host: String,

    /// MQTT broker port
    #[arg(long, default_value_t = 1883)]
    mqtt_port: u16,

    /// MQTT client ID
    #[arg(long, default_value = "mqttooth")]
    mqtt_client_id: String,

    /// Zigbee2MQTT device topic to subscribe to
    #[arg(long, default_value = "zigbee2mqtt/Temperature 1")]
    zigbee_topic: String,

    /// Device name for BLE advertising
    #[arg(long, default_value = "mqttooth")]
    device_name: String,
}

// BLE UUIDs (using standard Environmental Sensing service)
const ENVIRONMENTAL_SENSING_SERVICE_UUID: Uuid =
    Uuid::from_u128(0x0000181a_0000_1000_8000_00805f9b34fb);
const TEMPERATURE_CHAR_UUID: Uuid = Uuid::from_u128(0x00002a6e_0000_1000_8000_00805f9b34fb);

#[derive(Debug, Deserialize)]
struct TemperaturePayload {
    temperature: Option<f64>,
}

/// Shared state for the current temperature
#[derive(Debug, Default)]
struct AppState {
    current_temperature: f64,
}

async fn run_mqtt_client(
    state: Arc<RwLock<AppState>>,
    mqtt_options: MqttOptions,
    zigbee_topic: String,
) -> anyhow::Result<()> {
    let (client, mut eventloop) = AsyncClient::new(mqtt_options, 10);
    client.subscribe(&zigbee_topic, QoS::AtMostOnce).await?;
    info!("Subscribed to {}", zigbee_topic);

    loop {
        match eventloop.poll().await {
            Ok(Event::Incoming(Packet::Publish(publish))) => {
                let payload: TemperaturePayload = serde_json::from_slice(&publish.payload)?;
                let temperature = payload
                    .temperature
                    .ok_or_else(|| anyhow::anyhow!("Missing temperature field"))?;

                let mut state = state.write().await;
                state.current_temperature = temperature;

                info!("Temperature updated: {:.2}°C", temperature);
            }
            Ok(Event::Incoming(Packet::ConnAck(_))) => {
                info!("Connected to MQTT broker");
            }
            Ok(_) => {}
            Err(e) => return Err(e.into()),
        }
    }
}

/// Encode temperature as BLE sint16 (0.01 °C resolution)
fn encode_temperature(temperature: f64) -> Vec<u8> {
    let value = (temperature * 100.0) as i16;
    value.to_le_bytes().to_vec()
}

fn create_application(state: Arc<RwLock<AppState>>) -> Application {
    let characteristic_read = move |_req: CharacteristicReadRequest| {
        let state = state.clone();
        async move {
            let state = state.read().await;
            info!(
                "Read request, returning temperature: {:.2}°C",
                state.current_temperature
            );
            Ok(encode_temperature(state.current_temperature))
        }
        .boxed()
    };

    Application {
        services: vec![Service {
            uuid: ENVIRONMENTAL_SENSING_SERVICE_UUID,
            primary: true,
            characteristics: vec![Characteristic {
                uuid: TEMPERATURE_CHAR_UUID,
                read: Some(CharacteristicRead {
                    read: true,
                    fun: Box::new(characteristic_read),
                    ..Default::default()
                }),
                ..Default::default()
            }],
            ..Default::default()
        }],
        ..Default::default()
    }
}

async fn run_ble_server(state: Arc<RwLock<AppState>>, device_name: String) -> anyhow::Result<()> {
    let session = bluer::Session::new().await?;
    let adapter = session.default_adapter().await?;
    adapter.set_powered(true).await?;

    info!(
        "Advertising on Bluetooth adapter {} with address {}",
        adapter.name(),
        adapter.address().await?
    );

    let _app_handle = adapter
        .serve_gatt_application(create_application(state))
        .await?;

    // Set up advertising
    let le_advertisement = bluer::adv::Advertisement {
        service_uuids: vec![ENVIRONMENTAL_SENSING_SERVICE_UUID]
            .into_iter()
            .collect(),
        local_name: Some(device_name.clone()),
        discoverable: Some(true),
        ..Default::default()
    };

    let _adv_handle = adapter.advertise(le_advertisement).await?;
    info!("BLE GATT server started - advertising as '{}'", device_name);

    // Block forever
    std::future::pending().await
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let args = Args::parse();

    info!("Starting MQTTooth bridge...");
    info!(
        "Connecting to MQTT broker at {}:{}",
        args.mqtt_host, args.mqtt_port
    );

    let state = Arc::new(RwLock::new(AppState::default()));
    let mqtt_options = MqttOptions::new(args.mqtt_client_id, args.mqtt_host, args.mqtt_port);

    tokio::select! {
        result = run_mqtt_client(
            state.clone(),
            mqtt_options,
            args.zigbee_topic
        ) => result.context("MQTT client failed"),

        result = run_ble_server(state, args.device_name) => result.context("BLE server failed"),
        _ = tokio::signal::ctrl_c() => {
            info!("Shutting down...");
            Ok(())
        }
    }
}
