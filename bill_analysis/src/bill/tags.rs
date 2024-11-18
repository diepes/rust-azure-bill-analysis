use serde::Deserialize;
use serde::Deserializer; // used for custom tags deserialization
use std::collections::HashMap;
//use std::path::{Path, PathBuf};

// Tag data deserialized from the CSV file
#[derive(Debug)]
pub struct Tags {
    // for each lowercase key, we save the value of the tag and the original key(With case)
    pub kv: HashMap<String, (String, String)>,
    pub value: String,
}
impl Tags {
    pub fn to_lowercase(&mut self) -> Tags {
        Tags {
            kv: self.kv.clone(),
            value: self.value.to_lowercase(),
        }
    }
}

// Implement Deserialize for Tags, Vec<Tag>
impl<'de> Deserialize<'de> for Tags {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Deserialize the input into a string
        // e.g. '"JenkinsManagedTag": "ManagedByAzureVMAgents","JenkinsTemplateTag": "build-agent-azure"'
        let s = String::deserialize(deserializer)?;

        // Initialize a HashMap to hold the parsed tags
        let mut kv = HashMap::new();

        // Split the string by commas to separate each key-value pair
        for part in s.split(',') {
            // Split each pair by the colon to separate key and value.
            //let mut iter = part.split(": ");
            if let Some((key, value)) = part.split_once(": ") {
                // Trim quotes and whitespace and insert into the HashMap
                let k = key
                    .trim_start_matches('"')
                    .trim_end_matches('"')
                    .to_string(); // Search also done in lowercase
                let v = value
                    .trim_start_matches('"')
                    .trim_end_matches('"')
                    .to_string();

                // we save the value and original(case) key as a tuple
                kv.insert(k.to_lowercase(), (v, k));
            }
        }

        //println!("kv: {:?}", kv);
        // Return the Tags struct with the populated HashMap
        Ok(Tags {
            kv,
            value: s.to_lowercase(),
        })
    }
}
