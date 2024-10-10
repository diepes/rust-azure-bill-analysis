use serde::Deserialize;
use serde::Deserializer; // used for custom tags deserialization
use std::collections::HashMap;
use std::path::{Path, PathBuf};


macro_rules! lowercase_all_strings {
    ($struct:ident, $($field:ident),*) => {
        impl $struct {
            fn lowercase_all_strings(&mut self) {
                $(
                    self.$field = self.$field.to_lowercase();
                )*
            }
        }
    };
}

// Tag data deserialized from the CSV file
#[derive(Debug)]
pub struct Tags {
    pub kv: HashMap<String, String>,
    pub value: String,
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
            let mut iter = part.split(':');
            if let (Some(key), Some(value)) = (iter.next(), iter.next()) {
                // Trim quotes and whitespace and insert into the HashMap
                kv.insert(
                    key.trim_matches('"').trim().to_string(),
                    // double trim to remove all quotes
                    value.trim_matches('"').trim().trim_matches('"').to_string(),
                );
            }
        }

        // Return the Tags struct with the populated HashMap
        Ok(Tags { kv: kv, value: s })
    }
}
