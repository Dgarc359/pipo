use std::{
    collections::HashMap,
    fmt::Write,
    fs::File,
    path::Path,
    sync::{Arc, Mutex},
};

use axum::{Router, routing::get};

use tokio_stream::{StreamMap, wrappers::BroadcastStream};
use anyhow::anyhow;
use deadpool_sqlite::Pool;
use rand::{rngs::ThreadRng, thread_rng, RngCore};
use ruma::api::appservice::{Namespace, Namespaces, Registration, RegistrationInit};
use tokio::sync::broadcast;

use crate::http::Http;
use crate::{ConfigTransport, Message};

pub(crate) struct Matrix {
    transport_id: usize,
    registration: Registration,
    channels: HashMap<String, broadcast::Sender<Message>>,
    pipo_id: Arc<Mutex<i64>>,
    pool: Pool,
    listen_port: String,
}

impl Matrix {
    pub async fn new<P>(
        transport_id: usize,
        bus_map: &HashMap<String, broadcast::Sender<Message>>,
        pipo_id: Arc<Mutex<i64>>,
        pool: Pool,
        url: &str,
        registration_path: P,
        protocols: &Vec<ConfigTransport>,
        channel_mapping: &HashMap<String, String>,
        listen_port: String,
    ) -> anyhow::Result<Self>
    where
        P: AsRef<Path>,
    {
        let channels: HashMap<_, _> = channel_mapping
            .iter()
            .filter_map(|(channel_name, bus_name)| {
                if let Some(sender) = bus_map.get(bus_name) {
                    Some((channel_name.to_owned(), sender.clone()))
                } else {
                    eprintln!("No bus named '{}' in configuration file for channel name: '{}'", bus_name, channel_name);
                    None
                }
            })
            .collect();
        let protocols: Vec<_> = protocols.iter().map(|x| x.name().to_owned()).collect();
        let mut namespaces = Namespaces::new();
        dbg!("channels {:#?}", &channels);
        dbg!("protocols {:#?}", &protocols);
        /*
        namespaces.users = protocols
            .iter()
            .map(|x| Namespace::new(true, format!("@_{}_.*:tejat\\.net", x.to_lowercase())))
            .collect();
            */
        /*
        namespaces.users = protocols
            .iter()
            .map(|x| Namespace::new(true, "@.*:synapse".to_string()))
            .collect();
            */

        namespaces.aliases = protocols
            .iter()
            //.map(|x| Namespace::new(true, format!("#_{}_.#:tejat\\.net", x.to_lowercase())))
            .map(|x| Namespace::new(false, "t.*".to_string()))
            .collect();
        /*
        namespaces.aliases = protocols
            .iter()
            .map(|x| Namespace::new(true, format!("#_{}_.#:tejat\\.net", x.to_lowercase())))
            .collect();
            */
        namespaces.rooms = channels
            .keys()
            .map(|x| Namespace::new(true, x.to_owned()))
            .collect();

        let registration = match File::open(&registration_path) {
            Ok(registration_file) => {
                println!("matrix -- found registration file, pulling fields");
                let mut registration: Registration = serde_yaml::from_reader(registration_file)?;
                if &registration.id == "pipo" {
                    registration.url = Some(url.to_string());
                    registration.sender_localpart = "_pipo".to_string();
                    registration.namespaces = namespaces;
                    registration.rate_limited = Some(true);
                    registration.protocols = Some(protocols);
                    Ok(registration)
                } else {
                    Err(anyhow!(
                        "incorrect application service ID: \"{}\"",
                        registration.id
                    ))
                }
            },
            Err(e) => match e.kind() {
                std::io::ErrorKind::NotFound => {
                    println!("matrix -- did not find registration file, pulling fields");
                    let mut token_generator = TokenGenerator::new();
                    let as_token = token_generator.generate_token();
                    let hs_token = token_generator.generate_token();
                    let registration: Registration = RegistrationInit {
                        id: "pipo".to_string(),
                        url: Some(url.to_string()),
                        as_token,
                        hs_token,
                        sender_localpart: "_pipo".to_string(),
                        namespaces,
                        rate_limited: Some(true),
                        protocols: Some(protocols),
                    }
                    .into();
                    let registration_file = File::create(&registration_path)?;
                    serde_yaml::to_writer(registration_file, &registration)?;
                    Ok(registration)
                },
                _ => Err(e.into()),
            },
        }?;

        // when file exists, we're not writing, so let's just overwrite the file now
        // and then when I can figure out how to do this right, this can be
        // removed

        let fd = File::options().truncate(true).write(true).open(&registration_path).unwrap();
        serde_yaml::to_writer(fd, &registration)?;

        dbg!("late registration {:#?}", &registration);

        Ok(Self {
            transport_id,
            registration,
            channels,
            pipo_id,
            pool,
            listen_port,
        })
    }

    async fn serve_axum(&self) -> anyhow::Result<()> {
        println!("matrix -- creating routers and listeners");
        let mut http_router = Http::new(self.channels.clone());
        http_router.add_matrix_route(&self.registration.hs_token);
        let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", self.listen_port)).await.unwrap();
        println!("matrix -- created routers and listeners, serving");
        Ok(axum::serve(listener, http_router.app).await.unwrap())
    }

    fn create_input_buses(&self) -> StreamMap<String,BroadcastStream<Message>> {
        let mut input_buses = StreamMap::new();

        for (channel_name, channel) in self.channels.iter() {
            input_buses.insert(channel_name.clone(),
                BroadcastStream::new(channel.subscribe()));
        }

        input_buses
    }

    async fn connect_matrix(&self) -> anyhow::Result<
            StreamMap<String,BroadcastStream<Message>>
        > {
        self.serve_axum().await?;
        let input_buses = self.create_input_buses();

        Ok(input_buses)
    }

    pub async fn connect(&self) -> anyhow::Result<()> {
        println!("matrix -- connecting");
        let mut input_buses = self.connect_matrix().await?;

        loop {
            // stupid sexy infinite loop?
            tokio::select! {
                Some ((channel, message))
                    = tokio_stream::StreamExt::next(&mut input_buses) => {
                        let message = message.unwrap();
                        dbg!(&message);
                        return Ok(());
                    }
            }
        }
    }
}

struct TokenGenerator {
    random_bytes: [u8; 32],
    thread_rng: ThreadRng,
}

impl TokenGenerator {
    pub fn new() -> Self {
        Self {
            random_bytes: [0; 32],
            thread_rng: thread_rng(),
        }
    }
    pub fn generate_token(&mut self) -> String {
        self.thread_rng.fill_bytes(&mut self.random_bytes);
        let mut token = String::new();
        for byte in self.random_bytes {
            write!(token, "{byte:02x}").expect("failed to write to String");
        }
        token
    }
}
