use semver::{Comparator, VersionReq};
use thiserror::Error;

pub type Result<T> = core::result::Result<T, ConstError>;

#[derive(Error, Debug)]
pub enum ConstError {
    #[error("The version {0} provided for {1} is not valid: {2}")]
    VersionError(Comparator, String, &'static str),
    #[error("The max rust version {0} is not valid")]
    InvalidMaxRustVersionError(String),
    #[error("No satisfiable version of crate has a max version of {0}")]
    UnsatisfiableMaxRustVersionError(String),
    #[error("The version requirement for {crate_name}{crate_version} is empty")]
    EmptyVersionReqError {
        crate_name: String,
        crate_version: String,
    },
    #[error("Could not open file at {path}: {error}")]
    OpenFileError { path: String, error: std::io::Error },
    #[error("Error while fetching crate dependencies: {0}")]
    CrateDependencyFetchError(crates_io_api::Error),
    #[error("Error while fetching crate info: {0}")]
    CrateInfoFetchError(crates_io_api::Error),
    #[error("Could not parse version requirement: {0}")]
    VersionReqParseError(semver::Error),
    #[error("Could not parse version: {0}")]
    VersionParseError(semver::Error),
    #[error(
        "Invalid file contents, could not deserialize {type_name} from file at {path}: {error}"
    )]
    DeserializeFromFileError {
        type_name: &'static str,
        path: String,
        error: serde_cbor::Error,
    },
    #[error("Could not deserialize {type_name} into file at {path}: {error}")]
    SerializeToFileError {
        type_name: &'static str,
        path: String,
        error: serde_cbor::Error,
    },
    #[error("Could not get data directory")]
    DataDirectoryError,
    #[error("Could not create the {path} directory: {error}")]
    CreateParentDirectoryError { path: String, error: std::io::Error },
    #[error(
        "{}",
        display_non_overlapping_bounds_error(version_req, crate_name, crate_version)
    )]
    NonOverlappingBoundsError {
        version_req: String,
        crate_name: String,
        crate_version: String,
    },
    #[error("The crate {0} does not match any dependencies")]
    NoMatchingDependentError(String),
    #[error(
        "{}",
        display_unsatisfiable_multiple_dependent_error(crate_name, dependent, dependents)
    )]
    UnsatisfiableMultipleDependentsError {
        crate_name: String,
        dependent: ((String, String), VersionReq),
        dependents: Vec<((String, String), VersionReq)>,
    },
    #[error(
        "{}",
        display_unsatisfiable_bound_dependent_error(crate_name, lower, upper)
    )]
    UnsatisfiableBoundDependentsError {
        crate_name: String,
        lower: ((String, String), VersionReq),
        upper: ((String, String), VersionReq),
    },
    #[error(
        "{}",
        display_unsatisfiable_single_dependent_error(crate_name, dependent)
    )]
    UnsatisfiableSingleDependentError {
        crate_name: String,
        dependent: ((String, String), VersionReq),
    },
    #[error("Could not open lock file at {path}: {error}")]
    CouldNotLoadLockFileError {
        path: String,
        error: cargo_lock::Error,
    },
    #[error(
        "The crate {crate_name} has a prerelease version {crate_version} which is not supported"
    )]
    PreleaseVersionsNotSupported {
        crate_name: String,
        crate_version: String,
    },
    #[error("Only yanked version of crate {crate_name} satisfies the dependents requirements")]
    OnlyYankedVersionExistsError { crate_name: String },
    #[error(
        "The crate {crate_name} with version {crate_version} has the dependency {dependency} in the lockfile but crates.io says otherwise"
    )]
    DependencyMismatchFromCargoLock {
        crate_name: String,
        crate_version: String,
        dependency: String,
    },
    #[error("Expected \"all\" or a number, got {argument}")]
    InvalidCountArgument { argument: String },
}

fn display_non_overlapping_bounds_error(
    version_req: &String,
    crate_name: &String,
    crate_version: &String,
) -> String {
    format!(
        "The dependent crate {}{} provided has invalid version requirements {}",
        crate_name, crate_version, version_req
    )
}

fn display_unsatisfiable_single_dependent_error(
    crate_name: &String,
    dependent: &((String, String), VersionReq),
) -> String {
    format!(
        "The dependent crate {}{} provided with version requirements for {} of {} \n\
        does not match any vesions of {}",
        dependent.0 .0, dependent.0 .1, crate_name, dependent.0 .1, crate_name
    )
}

fn display_unsatisfiable_bound_dependent_error(
    crate_name: &String,
    lower: &((String, String), VersionReq),
    upper: &((String, String), VersionReq),
) -> String {
    format!(
        "\
        A version of the {} crate could not be selected due to {}{} with requirement {}\n\
        being incompatitible with {}{} with requirement {}, no version of {} satisfies those\
        requirements",
        crate_name, lower.0 .0, lower.0 .1, lower.1, upper.0 .0, upper.0 .1, upper.1, crate_name
    )
}

fn display_unsatisfiable_multiple_dependent_error(
    crate_name: &String,
    dependency: &((String, String), VersionReq),
    dependencies: &Vec<((String, String), VersionReq)>,
) -> String {
    let mut dependencies_as_string = String::new();

    for dependency in dependencies {
        dependencies_as_string.push_str(
            format!(
                "crate: {}{} with dependency requirement: {}\n",
                dependency.0 .0, dependency.0 .1, dependency.1
            )
            .as_str(),
        );
    }

    format!(
        "\
        A version of the {} crate could not be selected due to {}{} with requirement {}\n\
        being incompatitible with:-\n\
        {}\
        no version of {} matches those requirements",
        crate_name,
        dependency.0 .0,
        dependency.0 .1,
        dependency.1,
        dependencies_as_string,
        crate_name
    )
}

pub const NO_VERSION_BELOW: &str = "No version below";
pub const NO_VERSION_ABOVE: &str = "No version above";
pub const UNSUPPORTED_SEMVER_OPERATOR: &str = "Unsupported semver operator";
pub const PRELEASE_NOT_SUPPORTED: &str = "Prerelease versions are not supported";
