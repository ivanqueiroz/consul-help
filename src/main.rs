use clap::Parser;
use consulrs::{
    api::kv::requests::ReadKeyRequestBuilder,
    client::{ConsulClient, ConsulClientSettingsBuilder},
    kv,
};
use serde_yaml::Value;
use std::{collections::HashSet, io::Read, io::Write};
use std::{fs::File, path::PathBuf};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    app_prefix: String,

    #[arg(short, long, value_name = "INPUT PROPERTY")]
    input_property: Option<PathBuf>,

    #[arg(short, long)]
    consul_host: String,

    #[arg(short, long, value_name = "OUTPUT FILE")]
    output_file: Option<PathBuf>,

    #[arg(short, long, action = clap::ArgAction::Count)]
    debug: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ConsulProperties {
    pub key: String,
    pub value: String,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let result = load_consul_properties(&args.consul_host, &args.app_prefix).await;

    match &args.input_property {
        Some(input_file) => {
            let yml_properties = load_yml_properties(input_file);
            let difference = difference_between_properties(result, yml_properties);

            if difference.is_empty() {
                println!("No differences found.");
            } else {
                difference.iter().for_each(|item| {
                    println!("{}={}", item.key, item.value);
                });

                if let Some(output_file) = &args.output_file {
                    let mut file = File::create(output_file).expect("Unable to create file");
                    for item in difference {
                        let line = format!("{}={}\n", item.key, item.value);
                        file.write_all(line.as_bytes())
                            .expect("Unable to write to file");
                    }
                } else {
                    println!("No output file provided.");
                }
            }
        }
        None => {
            println!("No input property file provided.");
        }
    }
}

fn difference_between_properties(
    list1: Vec<ConsulProperties>,
    list2: Vec<ConsulProperties>,
) -> Vec<ConsulProperties> {
    let set1: HashSet<_> = list1.iter().cloned().collect();
    let set2: HashSet<_> = list2.iter().cloned().collect();

    let difference: HashSet<_> = set1.difference(&set2).cloned().collect();

    difference.into_iter().collect()
}

fn load_yml_properties(file_path: &PathBuf) -> Vec<ConsulProperties> {
    println!("Loading properties from file: {}", file_path.display());

    let mut file = File::open(file_path).expect("Unable to open file");
    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .expect("Unable to read file");

    let yaml: Value = serde_yaml::from_str(&contents).expect("Unable to parse YAML");
    let mut result = Vec::new();
    flatten_yaml(&yaml, &mut result, String::new());

    let properties: Vec<ConsulProperties> = result
        .into_iter()
        .map(|item| ConsulProperties {
            key: item.0,
            value: item.1,
        })
        .collect();
    properties
}

fn flatten_yaml(value: &Value, properties: &mut Vec<(String, String)>, prefix: String) {
    match value {
        Value::Mapping(mapping) => {
            for (key, value) in mapping {
                if let Value::String(key_str) = key {
                    let new_prefix = if prefix.is_empty() {
                        key_str.clone()
                    } else {
                        format!("{}/{}", prefix, key_str)
                    };
                    flatten_yaml(value, properties, new_prefix);
                }
            }
        }
        Value::Sequence(sequence) => {
            for (index, value) in sequence.iter().enumerate() {
                let new_prefix = format!("{}[{}]", prefix, index);
                flatten_yaml(value, properties, new_prefix);
            }
        }
        _ => {
            properties.push((prefix, value_to_string(value)));
        }
    }
}

fn value_to_string(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        Value::Sequence(seq) => format!("{:?}", seq),
        Value::Mapping(map) => format!("{:?}", map),
        Value::Tagged(tagged) => format!("{:?}", tagged),
    }
}

async fn load_consul_properties(consul_host: &str, app_prefix: &str) -> Vec<ConsulProperties> {
    println!(
        "Loading properties from consul host: {} to key {}",
        consul_host, app_prefix
    );
    let consul_address = format!("http://{}:8500", consul_host);
    let client = ConsulClient::new(
        ConsulClientSettingsBuilder::default()
            .address(consul_address)
            .verify(false)
            .build()
            .unwrap(),
    )
    .unwrap();

    let mut read_request = ReadKeyRequestBuilder::default();
    read_request.key(app_prefix).recurse(true);

    let res = kv::read(&client, app_prefix, Some(&mut read_request))
        .await
        .unwrap();

    let prefix = String::from(app_prefix);

    let prefix = prefix + "/";

    res.response
        .into_iter()
        .map(|item| ConsulProperties {
            key: item.key.replace(&prefix, ""),
            value: item.value.unwrap().try_into().unwrap(),
        })
        .collect()
}
