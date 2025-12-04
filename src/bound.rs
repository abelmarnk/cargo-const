use cargo_lock::Lockfile;
use semver::{BuildMetadata, Comparator, Op, Prerelease, Version, VersionReq};
use std::{
    mem::take,
    ops::{Add, Sub},
    u64,
};

use crate::{
    error::{ConstError, Result, UNSUPPORTED_SEMVER_OPERATOR},
    provider::{ParsedVersion, Provider},
    utils::CRATE_NAME,
};

// Get a bound for the crate based on the dependent's requirements as well as all versions
// of that crate
// It combines all the requirements from direct dependents into one single range and then
// matches that range against actual versions of the crate.
// The method for finding it is simple and works in most cases, but it doesn't:-
// - Take into account disjoint dependencies, i.e when there are two disjoint versions
//   of the crate being used by two dependents that don't interact, the approach
//   here flags them as incompatible.
// - Attempt to bump other dependents down in order to find more compatitible versions
//   this would end up changing the versions of other dependencies, would be slower to
//   find and would be harder on crates.io, so if at all it is added it would be gated.

pub fn find_packed_bound(
    client: &Provider,
    crate_to_find: &str,
    lock: &Lockfile,
) -> Result<((usize, usize), Vec<ParsedVersion>)> {
    // Find all dependent packages that depend on `crate_to_find`, picking out the name and version
    let dependents = lock
        .packages
        .iter()
        .filter(|package| {
            package
                .dependencies
                .iter()
                .find(|dependency| dependency.name.as_str().eq(crate_to_find))
                .is_some()
                && package.name.as_str().ne(CRATE_NAME)
        })
        .map(|package| {
            (
                package.name.as_str().to_string(),
                package.version.to_string(),
            )
        });

    // Find all the dependency constraints set by the dependents, picking out the name, version
    // and constraint for that crate
    let dependent_constraints = dependents
        .map(|mut some_crate| {
            let result = client.get_dependencies(&some_crate.0, &some_crate.1);

            let parsed_dependencies = result?;

            let parsed_dependency = parsed_dependencies
                .dependencies
                .into_iter()
                .find(|parsed_dependency| parsed_dependency.crate_id.eq(crate_to_find))
                .ok_or_else(|| ConstError::DependencyMismatchFromCargoLock {
                    dependency: crate_to_find.to_string(),
                    crate_name: take(&mut some_crate.0), // We use can take because we short circuit below
                    crate_version: take(&mut some_crate.1),
                })?;

            Ok((some_crate, parsed_dependency))
        })
        .collect::<Result<Vec<_>>>()?;

    if dependent_constraints.is_empty() {
        return Err(ConstError::NoMatchingDependentError(
            crate_to_find.to_string(),
        ));
    }

    // Add the bound to the above so it becomes the name, version, constraint and bound
    let mut dependent_constraints = dependent_constraints
        .into_iter()
        .map(|mut dep| {
            let mut result = Bound::try_from(&dep.1.version_req);

            if let Err(error) = result.as_mut() {
                if let ConstError::NonOverlappingBoundsError {
                    version_req: _,
                    crate_name,
                    crate_version,
                } = error
                {
                    // We short-circuit below so we can take the string
                    *crate_name = take(&mut dep.0 .0);
                    *crate_version = take(&mut dep.0 .1);
                } else if let ConstError::EmptyVersionReqError {
                    crate_name,
                    crate_version,
                } = error
                {
                    // We short-circuit below so we can take the string
                    *crate_name = take(&mut dep.0 .0);
                    *crate_version = take(&mut dep.0 .1);
                }
            }

            let bound = result?;

            Ok((dep.0, bound, dep.1.version_req))
        })
        .collect::<Result<Vec<_>>>()?;

    let first = &dependent_constraints.first().unwrap().1;

    let (lower_range, upper_range) = (
        (&first.lower.version, first.lower.inclusive),
        (&first.upper.version, first.upper.inclusive),
    );

    let mut upper_index = 0;

    let mut lower_index = 0;

    // Find the overlap between all bounds or find the index with the first conflict
    let result: std::result::Result<((&Version, bool), (&Version, bool)), usize> =
        dependent_constraints.iter().skip(1).enumerate().try_fold(
            (lower_range, upper_range),
            |mut value_1, value_2| {
                if contains_from_lower(value_1.0, &value_2.1 .1.lower)
                    .eq(&Ordering::ContainsFromLower)
                {
                    lower_index = value_2.0.add(1);
                    value_1.0 = (&value_2.1 .1.lower.version, value_2.1 .1.lower.inclusive);
                }

                if contains_from_upper(value_1.1, &value_2.1 .1.upper)
                    .eq(&Ordering::ContainsFromUpper)
                {
                    upper_index = value_2.0.add(1);
                    value_1.1 = (&value_2.1 .1.upper.version, value_2.1 .1.upper.inclusive);
                }

                if contains_from_upper(value_1.0, value_1.1).eq(&Ordering::ContainsFromUpper) {
                    return Err(value_2.0.add(1));
                }

                Ok(value_1)
            },
        );

    match result {
        // At this point bound.lower <= bound.upper now we just have to make sure that
        // that bound matches one or more actual versions
        Ok(bound) => {
            let mut versions = client.get_versions(crate_to_find)?.versions;

            versions.sort();

            let lower = match versions.binary_search_by(|version| version.num.cmp(&bound.0 .0)) {
                Ok(value) => {
                    if bound.0 .1 {
                        value
                    } else {
                        value.add(1)
                    }
                }
                Err(value) => value,
            };

            let lower = isize::try_from(lower).unwrap();

            let upper = match versions.binary_search_by(|version| version.num.cmp(&bound.1 .0)) {
                Ok(value) => {
                    let value = isize::try_from(value).unwrap();
                    if bound.1 .1 {
                        value
                    } else {
                        value.sub(1)
                    }
                }
                Err(value) => {
                    // We convert to isize so we can go below 0.
                    isize::try_from(value).unwrap().sub(1)
                }
            };

            if lower.gt(&upper) {
                if lower_index.eq(&upper_index) {
                    let bound = dependent_constraints
                        .get_mut(usize::from(lower_index))
                        .unwrap();

                    Err(ConstError::UnsatisfiableSingleDependentError {
                        crate_name: crate_to_find.to_string(),
                        dependent: (take(&mut bound.0), take(&mut bound.2)),
                    })
                } else {
                    let lower = (
                        take(&mut dependent_constraints.get_mut(lower_index).unwrap().0),
                        take(&mut dependent_constraints.get_mut(lower_index).unwrap().2),
                    );
                    let upper = (
                        take(&mut dependent_constraints.get_mut(upper_index).unwrap().0),
                        take(&mut dependent_constraints.get_mut(upper_index).unwrap().2),
                    );

                    Err(ConstError::UnsatisfiableBoundDependentsError {
                        crate_name: crate_to_find.to_string(),
                        lower,
                        upper,
                    })
                }
            } else {
                let lower = usize::try_from(lower).unwrap();
                let upper = usize::try_from(upper).unwrap();

                Ok(((lower, upper), versions))
            }
        }
        // The last dependent which we tried to resolve their requirement caused the solution to
        // be unsatisfiable, we find all the dependents that would make it as such
        Err(index) => {
            let mut unmet = Vec::new();

            // TODO: Rather than allocating a new vector use the old one, and use a
            // swap to avoid moving many elements

            let bound = dependent_constraints.get(index).unwrap().1.clone();

            // Find all the unsatisfiable dependents
            for value in dependent_constraints
                .iter_mut()
                .enumerate()
                .filter(|(position, _)| position.ne(&index))
            {
                if contains_from_upper(&value.1 .1.lower, &bound.upper)
                    .eq(&Ordering::ContainsFromUpper)
                    || contains_from_lower(&value.1 .1.upper, &bound.lower)
                        .eq(&Ordering::ContainsFromLower)
                {
                    unmet.push((take(&mut value.1 .0), take(&mut value.1 .2)));
                }
            }

            let bound = dependent_constraints.get_mut(index).unwrap();

            Err(ConstError::UnsatisfiableMultipleDependentsError {
                crate_name: crate_to_find.to_string(),
                dependent: (take(&mut bound.0), take(&mut bound.2)),
                dependents: unmet,
            })
        }
    }
}

#[derive(Clone)]
pub struct Range {
    pub version: Version,
    pub inclusive: bool,
}

impl<'a> From<&'a Range> for (&'a Version, bool) {
    fn from<'b>(value: &'b Range) -> (&'b Version, bool) {
        (&value.version, value.inclusive)
    }
}

// std::cmp::Ordering could be used but the
// equals case(and then taking into account the is_inclusive case)
// could very easily be a source of confusion and at that
// point stops being analogical to it, so this is used instead
#[derive(PartialEq, Eq)]
pub enum Ordering {
    ContainsFromLower,
    ContainsFromUpper,
    Equal,
}

fn contains_from_lower<'c, 'd, T, R>(a: T, b: R) -> Ordering
where
    T: Into<(&'c Version, bool)>,
    R: Into<(&'d Version, bool)>,
{
    let a = a.into();
    let b = b.into();
    match a.0.cmp_precedence(&b.0) {
        std::cmp::Ordering::Less => Ordering::ContainsFromLower,
        std::cmp::Ordering::Equal => {
            if !b.1 && a.1 {
                Ordering::ContainsFromLower
            } else if b.1 && !a.1 {
                Ordering::ContainsFromUpper
            } else {
                Ordering::Equal
            }
        }
        std::cmp::Ordering::Greater => Ordering::ContainsFromUpper,
    }
}

fn contains_from_upper<'c, 'd, T, R>(a: T, b: R) -> Ordering
where
    T: Into<(&'c Version, bool)>,
    R: Into<(&'d Version, bool)>,
{
    let a = a.into();
    let b = b.into();
    match a.0.cmp_precedence(&b.0) {
        std::cmp::Ordering::Less => Ordering::ContainsFromLower,
        std::cmp::Ordering::Equal => {
            if !b.1 && a.1 {
                Ordering::ContainsFromUpper
            } else if b.1 && !a.1 {
                Ordering::ContainsFromLower
            } else {
                Ordering::Equal
            }
        }
        std::cmp::Ordering::Greater => Ordering::ContainsFromUpper,
    }
}

#[derive(Clone)]
pub struct Bound {
    pub upper: Range,
    pub lower: Range,
}

impl TryFrom<&VersionReq> for Bound {
    type Error = ConstError;

    // If the version req did not match any valid versions of that crate then
    // the same would go for the Bound, it only guarantees that it captures
    // the version req

    // For the two errors below, i am conflicted between this and just returning a
    // similarly unsatisfiable bound that would capture the fact that the version req
    // captures nothing, in both cases it would still be up to the caller to verify
    // what is returned in this case they have to modify the error in the other case
    // they would have to check that Bound::lower <= Bound::upper, for now i will
    // stick with this
    fn try_from(version_req: &VersionReq) -> Result<Self> {
        let mut comparators = version_req.comparators.iter();

        let first = comparators
            .next()
            .ok_or_else(|| ConstError::EmptyVersionReqError {
                crate_name: String::default(),
                crate_version: String::default(),
            })?;

        let err_closure = || ConstError::NonOverlappingBoundsError {
            version_req: version_req.to_string(),
            crate_name: String::default(),
            crate_version: String::default(),
        };

        let Bound { upper, lower } = Bound::try_from(first)?;
        let (lower, upper) = (
            (lower.version, lower.inclusive),
            (upper.version, upper.inclusive),
        );

        let bound = comparators.try_fold((lower, upper), |acc, next| {
            Bound::try_from(next).and_then(|next| {
                let (lower, upper) = (
                    if contains_from_lower((&acc.0 .0, acc.0 .1), &next.lower)
                        .eq(&Ordering::ContainsFromLower)
                    {
                        (next.lower.version, next.lower.inclusive)
                    } else {
                        (acc.0 .0, acc.0 .1)
                    },
                    if contains_from_upper((&acc.1 .0, acc.1 .1), &next.upper)
                        .eq(&Ordering::ContainsFromUpper)
                    {
                        (next.upper.version, next.upper.inclusive)
                    } else {
                        (acc.1 .0, acc.1 .1)
                    },
                );

                if contains_from_upper((&lower.0, lower.1), (&upper.0, upper.1))
                    .eq(&Ordering::ContainsFromUpper)
                {
                    Err(err_closure())
                } else {
                    Ok((lower, upper))
                }
            })
        })?;

        Ok(Bound {
            lower: Range {
                version: bound.0 .0,
                inclusive: bound.0 .1,
            },
            upper: Range {
                version: bound.1 .0,
                inclusive: bound.1 .1,
            },
        })
    }
}

impl From<&Bound> for VersionReq {
    fn from(bound: &Bound) -> Self {
        let lower_comparator = Comparator {
            op: if bound.lower.inclusive {
                Op::GreaterEq
            } else {
                Op::Greater
            },
            major: bound.lower.version.major,
            minor: Some(bound.lower.version.minor),
            patch: Some(bound.lower.version.patch),
            pre: bound.lower.version.pre.clone(),
        };

        let upper_comparator = Comparator {
            op: if bound.lower.inclusive {
                Op::LessEq
            } else {
                Op::Less
            },
            major: bound.upper.version.major,
            minor: Some(bound.upper.version.minor),
            patch: Some(bound.upper.version.patch),
            pre: bound.upper.version.pre.clone(),
        };

        VersionReq {
            comparators: vec![lower_comparator, upper_comparator],
        }
    }
}

impl TryFrom<&Comparator> for Bound {
    type Error = ConstError;
    fn try_from(comparator: &Comparator) -> Result<Self> {
        match comparator.op {
            Op::Caret => Ok(Bound {
                lower: Range {
                    version: Version {
                        major: comparator.major,
                        minor: comparator.minor.unwrap_or(0),
                        patch: comparator.patch.unwrap_or(0),
                        pre: comparator.pre.clone(),
                        build: BuildMetadata::EMPTY,
                    },
                    inclusive: true,
                },
                upper: Range {
                    version: Version {
                        major: comparator.major.add(1),
                        minor: 0,
                        patch: 0,
                        pre: Prerelease::EMPTY,
                        build: BuildMetadata::EMPTY,
                    },
                    inclusive: false,
                },
            }),
            Op::Tilde => Ok(Bound {
                lower: Range {
                    version: Version {
                        major: comparator.major,
                        minor: comparator.minor.unwrap_or(0),
                        patch: comparator.patch.unwrap_or(0),
                        pre: comparator.pre.clone(),
                        build: BuildMetadata::EMPTY,
                    },
                    inclusive: true,
                },
                upper: Range {
                    version: Version {
                        major: comparator.major,
                        minor: comparator.minor.unwrap_or(0).add(1),
                        patch: 0,
                        pre: Prerelease::EMPTY,
                        build: BuildMetadata::EMPTY,
                    },
                    inclusive: false,
                },
            }),
            Op::Exact => Ok(Bound {
                lower: Range {
                    version: Version {
                        major: comparator.major,
                        minor: comparator.minor.unwrap_or(0),
                        patch: comparator.patch.unwrap_or(0),
                        pre: comparator.pre.clone(),
                        build: BuildMetadata::EMPTY,
                    },
                    inclusive: true,
                },
                upper: Range {
                    version: Version {
                        major: comparator.major,
                        minor: comparator.minor.unwrap_or(0),
                        patch: comparator.patch.unwrap_or(0),
                        pre: comparator.pre.clone(),
                        build: BuildMetadata::EMPTY,
                    },
                    inclusive: true,
                },
            }),
            Op::Greater => Ok(Bound {
                lower: Range {
                    version: Version {
                        major: comparator.major,
                        minor: comparator.minor.unwrap_or(0),
                        patch: comparator.patch.unwrap_or(0),
                        pre: comparator.pre.clone(),
                        build: BuildMetadata::EMPTY,
                    },
                    inclusive: false,
                },
                upper: Range {
                    version: Version {
                        major: u64::MAX,
                        minor: u64::MAX,
                        patch: u64::MAX,
                        pre: Prerelease::EMPTY,
                        build: BuildMetadata::EMPTY,
                    },
                    inclusive: true,
                },
            }),
            Op::GreaterEq => Ok(Bound {
                lower: Range {
                    version: Version {
                        major: comparator.major,
                        minor: comparator.minor.unwrap_or(0),
                        patch: comparator.patch.unwrap_or(0),
                        pre: comparator.pre.clone(),
                        build: BuildMetadata::EMPTY,
                    },
                    inclusive: true,
                },
                upper: Range {
                    version: Version {
                        major: u64::MAX,
                        minor: u64::MAX,
                        patch: u64::MAX,
                        pre: Prerelease::EMPTY,
                        build: BuildMetadata::EMPTY,
                    },
                    inclusive: true,
                },
            }),
            Op::Less => Ok(Bound {
                lower: Range {
                    version: Version {
                        major: 0,
                        minor: 0,
                        patch: 0,
                        pre: Prerelease::EMPTY,
                        build: BuildMetadata::EMPTY,
                    },
                    inclusive: true,
                },
                upper: Range {
                    version: Version {
                        major: comparator.major,
                        minor: comparator.minor.unwrap_or(0),
                        patch: comparator.patch.unwrap_or(0),
                        pre: comparator.pre.clone(),
                        build: BuildMetadata::EMPTY,
                    },
                    inclusive: false,
                },
            }),
            Op::LessEq => Ok(Bound {
                lower: Range {
                    version: Version {
                        major: 0,
                        minor: 0,
                        patch: 0,
                        pre: Prerelease::EMPTY,
                        build: BuildMetadata::EMPTY,
                    },
                    inclusive: true,
                },
                upper: Range {
                    version: Version {
                        major: comparator.major,
                        minor: comparator.minor.unwrap_or(0),
                        patch: comparator.patch.unwrap_or(0),
                        pre: comparator.pre.clone(),
                        build: BuildMetadata::EMPTY,
                    },
                    inclusive: true,
                },
            }),
            Op::Wildcard => Ok(Bound {
                lower: Range {
                    version: Version {
                        major: 0,
                        minor: 0,
                        patch: 0,
                        pre: Prerelease::EMPTY,
                        build: BuildMetadata::EMPTY,
                    },
                    inclusive: true,
                },
                upper: Range {
                    version: Version {
                        major: u64::MAX,
                        minor: u64::MAX,
                        patch: u64::MAX,
                        pre: Prerelease::EMPTY,
                        build: BuildMetadata::EMPTY,
                    },
                    inclusive: true,
                },
            }),
            _ => Err(ConstError::VersionError(
                comparator.clone(),
                String::default(),
                UNSUPPORTED_SEMVER_OPERATOR,
            )),
        }
    }
}
