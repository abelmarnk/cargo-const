use cargo_lock::Lockfile;
use clap::Parser;
use std::str::FromStr;

use crate::{
    bound::find_packed_bound,
    error::{ConstError, Result},
    provider::Provider,
    utils::{get_rust_version, print_header_and_items},
};

#[derive(Debug)]
pub enum Count {
    All,
    Count(usize),
}

impl FromStr for Count {
    type Err = ConstError;
    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value {
            "all" => Ok(Count::All),
            value => {
                let count = value
                    .parse()
                    .map_err(|_| ConstError::InvalidCountArgument {
                        argument: value.to_string(),
                    })?;
                Ok(Count::Count(count))
            }
        }
    }
}

/// Find all versions of a crate compatible with the project's dependencies
#[derive(Parser)]
pub struct Compat {
    /// Whether or not to include yanked versions
    #[clap(short, long)]
    include_yanked: bool,
    /// List out versions instead of using a range
    #[clap(short, long, default_value = "5")]
    count: Count,
    /// Path to cargo.lock
    #[clap(short, long, default_value = "Cargo.lock")]
    path: String,
    /// Max rust version supported
    #[clap(short, long)]
    max_version: Option<String>,
    /// Dependency to find minimum version of
    dependency: String,
}

impl Compat {
    pub fn run(self) -> Result<()> {
        let lock =
            Lockfile::load(&self.path).map_err(|error| ConstError::CouldNotLoadLockFileError {
                path: self.path,
                error,
            })?;

        let provider = Provider::new();

        // Find the range and get all versions of the crate sorted
        let ((lower_bound, upper_bound), versions) =
            find_packed_bound(&provider, &self.dependency, &lock)?;

        let count = match self.count {
            Count::All => usize::MAX,
            Count::Count(count) => count,
        };

        let versions = versions.iter().take(upper_bound).skip(lower_bound).rev(); // Display later versions first

        let versions = versions.filter(|version| self.include_yanked || !version.yanked);

        if versions.clone().peekable().peek().is_none() {
            return Err(ConstError::OnlyYankedVersionExistsError {
                crate_name: self.dependency,
            });
        }

        let versions: Box<dyn Iterator<Item = _>> = if let Some(version_str) = &self.max_version {
            if let Some(version) = get_rust_version(&version_str) {
                let versions = versions.filter(move |crate_version| {
                    if let Some(ref crate_rust_version) = crate_version.rust_version {
                        if let Some(crate_rust_version) = get_rust_version(crate_rust_version) {
                            crate_rust_version.le(&version)
                        } else {
                            true
                        }
                    } else {
                        true
                    }
                });

                if versions.clone().peekable().peek().is_none() {
                    return Err(ConstError::UnsatisfiableMaxRustVersionError(
                        version_str.to_owned(),
                    ));
                }

                Box::new(versions)
            } else {
                return Err(ConstError::InvalidMaxRustVersionError(
                    version_str.to_owned(),
                ));
            }
        } else {
            Box::new(versions)
        };

        let versions = versions.take(count).map(|version| {
            let min_rust_version_message = version
                .rust_version
                .as_ref()
                .map(|version| format!("    min-rust-version = {}", version))
                .unwrap_or_default();
            format!("{}{}", &version.num, min_rust_version_message)
        });

        print_header_and_items("Compatible versions found", versions);

        Ok(())
    }
}
