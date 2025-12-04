use std::{
    fs::OpenOptions,
    path::{Path, PathBuf},
    time::Duration,
};

use crates_io_api::{SyncClient, Version as CratesIoVersion};
use semver::{Version as SemverVersion, VersionReq};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::{
    error::{ConstError, Result},
    utils::{
        now_as_secs, print_info, print_warning, CRATE_NAME, CRATE_VERSION, MAX_CACHE_AGE,
        MY_USER_AGENT,
    },
};

#[derive(Deserialize, Serialize, Clone)]
pub struct ParsedDependency {
    pub crate_id: String,
    pub version_req: VersionReq,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct ParsedCrateDependency {
    pub dependencies: Vec<ParsedDependency>,
}

#[derive(Deserialize, Serialize, Ord, Eq)]
pub struct ParsedVersion {
    pub yanked: bool,
    pub num: SemverVersion,
    pub rust_version: Option<String>,
}

impl PartialOrd for ParsedVersion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        if self.num.lt(&other.num) {
            Some(std::cmp::Ordering::Less)
        } else if self.num.eq(&other.num) {
            Some(std::cmp::Ordering::Equal)
        } else {
            Some(std::cmp::Ordering::Greater)
        }
    }
}

impl PartialEq for ParsedVersion {
    fn eq(&self, other: &Self) -> bool {
        self.num.eq(&other.num)
    }
}

#[derive(Deserialize, Serialize)]
pub struct ParsedCrateVersion {
    pub versions: Vec<ParsedVersion>,
}

pub struct Provider {
    client: SyncClient,
}

impl Provider {
    pub fn new() -> Provider {
        let client = SyncClient::new(MY_USER_AGENT, Duration::from_millis(100)).unwrap();

        Provider { client }
    }

    pub fn get_dependencies(
        &self,
        crate_name: &str,
        crate_version: &str,
    ) -> Result<ParsedCrateDependency> {
        let mut data_dir = get_data_location();

        if let Some(data_dir) = data_dir.as_mut() {
            data_dir.push("dependencies");
            data_dir.push(crate_name);
            data_dir.push(crate_version);

            if let Ok((cache_time, crate_dependencies)) = read_from_file::<_, (u64, _)>(data_dir) {
                if cache_time.gt(&(now_as_secs() - MAX_CACHE_AGE)) {
                    return Ok(crate_dependencies);
                }
            }
        };

        let dependencies = self
            .client
            .crate_dependencies(crate_name, crate_version)
            .map_err(|error| ConstError::CrateDependencyFetchError(error))?;

        let result = dependencies
            .into_iter()
            .map(|dependency| {
                let crates_io_api::Dependency { crate_id, req, .. } = dependency;

                Ok(ParsedDependency {
                    crate_id,
                    version_req: VersionReq::parse(&req)
                        .map_err(|error| ConstError::VersionReqParseError(error))?,
                })
            })
            .collect::<Result<Vec<ParsedDependency>>>();

        let parsed_crate_dependencies = ParsedCrateDependency {
            dependencies: result?,
        };

        match data_dir.as_ref() {
            Some(data_dir) => {
                let result = write_to_file(data_dir, (now_as_secs(), &parsed_crate_dependencies));
                if result.is_err() {
                    let message = format!(
                        "Could not create cache at {:?}\n{}",
                        data_dir,
                        "Repeated requests without caching increases chances of rate limiting"
                    );

                    print_warning(&message);
                } else {
                    let message = format!("Cache succesfully created at {:?}", data_dir);

                    print_info(&message);
                }
            }
            None => {
                let message = format!(
                    "Could not access directory at {:?}\n{}",
                    data_dir,
                    "Repeated requests without caching increases chances of rate limiting"
                );

                print_warning(&message);
            }
        }

        Ok(parsed_crate_dependencies)
    }

    pub fn get_versions(&self, crate_to_find: &str) -> Result<ParsedCrateVersion> {
        let mut data_dir = get_data_location();

        if let Some(data_dir) = data_dir.as_mut() {
            data_dir.push("versions");
            data_dir.push(crate_to_find);

            if let Ok((cache_time, crate_versions)) = read_from_file::<_, (u64, _)>(data_dir) {
                if cache_time.gt(&(now_as_secs() - MAX_CACHE_AGE)) {
                    return Ok(crate_versions);
                }
            }
        };

        let result = self
            .client
            .get_crate(crate_to_find)
            .map_err(|error| ConstError::CrateInfoFetchError(error))?;

        let result = result
            .versions
            .into_iter()
            .map(|version| {
                let CratesIoVersion {
                    num,
                    yanked,
                    rust_version,
                    ..
                } = version;

                let semver_version = SemverVersion::parse(&num)
                    .map_err(|error| ConstError::VersionParseError(error))?;

                Ok(ParsedVersion {
                    num: semver_version,
                    yanked,
                    rust_version,
                })
            })
            .collect::<Result<Vec<ParsedVersion>>>();

        let parsed_crate_versions = ParsedCrateVersion { versions: result? };

        match data_dir.as_ref() {
            Some(data_dir) => {
                let result = write_to_file(data_dir, (now_as_secs(), &parsed_crate_versions));
                if result.is_err() {
                    let message = format!(
                        "Could not create cache at {:?}\n{}",
                        data_dir,
                        "Repeated requests without caching increases chances of rate limiting"
                    );

                    print_warning(&message);
                } else {
                    let message = format!("Cache succesfully created at {:?}", data_dir);

                    print_info(&message);
                }
            }
            None => {
                let message = format!(
                    "Could not access directory at {:?}\n{}",
                    data_dir,
                    "Repeated requests without caching increases chances of rate limiting"
                );

                print_warning(&message);
            }
        }

        Ok(parsed_crate_versions)
    }
}

fn read_from_file<P, T>(path: P) -> Result<T>
where
    T: DeserializeOwned,
    P: AsRef<Path>,
{
    let result = std::fs::File::open(path.as_ref()).map_err(|error| ConstError::OpenFileError {
        path: path.as_ref().to_string_lossy().to_string(),
        error,
    });

    let file = result?;

    let buffer = std::io::BufReader::new(file);

    let result = serde_cbor::from_reader::<T, _>(buffer).map_err(|error| {
        ConstError::DeserializeFromFileError {
            type_name: std::any::type_name::<T>(),
            path: path.as_ref().to_string_lossy().to_string(),
            error,
        }
    });

    result
}

fn write_to_file<P, T>(path: P, value: T) -> Result<()>
where
    P: AsRef<Path>,
    T: Serialize,
{
    if let Some(parent) = path.as_ref().parent() {
        if !parent.exists() {
            if let Err(error) = std::fs::create_dir_all(parent) {
                return Err(ConstError::CreateParentDirectoryError {
                    path: parent.to_string_lossy().to_string(),
                    error,
                });
            }
            // Log that the parent directory is created
        }
    }
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .open(&path)
        .map_err(|error| ConstError::OpenFileError {
            path: path.as_ref().to_string_lossy().to_string(),
            error,
        })?;

    let writer = std::io::BufWriter::new(file);

    let result =
        serde_cbor::to_writer(writer, &value).map_err(|error| ConstError::SerializeToFileError {
            type_name: std::any::type_name::<T>(),
            path: path.as_ref().to_string_lossy().to_string(),
            error,
        });

    result
}

fn get_data_location() -> Option<PathBuf> {
    let mut data_dir = dirs::data_dir();

    if let Some(data_dir) = data_dir.as_mut() {
        data_dir.push(format!("{}-{}", CRATE_NAME, CRATE_VERSION));
    }

    data_dir
}
