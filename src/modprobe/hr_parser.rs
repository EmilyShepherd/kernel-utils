use super::ModuleDir;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Error, ErrorKind, Result, Seek, SeekFrom};

/// Parses the human readable modules.alias as produced by depmod
pub struct AliasParser {
    aliases: HashMap<String, Vec<String>>,
}

impl AliasParser {
    /// Creates a new AliasParser by opening its preferred file inside the given
    /// [ModuleDir].
    pub fn new(dir: &ModuleDir) -> Result<Self> {
        let reader = BufReader::new(dir.open("modules.alias")?);
        let mut aliases: HashMap<String, Vec<String>> = HashMap::new();

        // We need to eagerly parse the entire alias file, because aliases can
        // map to more than one module, and scanning the a human readable file
        // is too slow to do over and over.
        for line in reader.lines() {
            let line = line?;
            let mut line = line.split(' ');

            if !line.next().is_some_and(|x| x == "alias") {
                continue;
            }

            let alias = line
                .next()
                .ok_or(Error::new(ErrorKind::Other, "Malformed modules.alias file"))?
                .replace("-", "_");

            let module = line
                .next()
                .ok_or(Error::new(ErrorKind::Other, "Malformed modules.alias file"))?
                .to_string();

            if let Some(existing) = aliases.get_mut(&alias) {
                existing.push(module);
            } else {
                aliases.insert(alias, vec![module]);
            }
        }

        Ok(Self { aliases: aliases })
    }

    /// Looks up the given alias and returns the names of any modules it maps
    /// to.
    pub fn lookup(&self, name: &str) -> Option<&Vec<String>> {
        self.aliases.get(&name.replace("-", "_"))
    }
}

/// Parses the human readable modules.dep file as produced by depmod
pub struct DepParser {
    deps: File,
}

impl DepParser {
    /// Creates a new DepParser by opening its preferred file inside the given
    /// [ModuleDir].
    pub fn new(dir: &ModuleDir) -> Result<Self> {
        Ok(Self {
            deps: dir.open("modules.dep")?,
        })
    }

    /// Looks up the given module and returns all the module files that must be
    /// loaded to satisfy it.
    pub fn lookup(&mut self, module: &str) -> Result<Option<Vec<String>>> {
        self.deps.seek(SeekFrom::Start(0))?;
        let reader = BufReader::new(&self.deps);
        let matcher = format!("{}.ko", module.replace("-", "_"));

        for line in reader.lines() {
            let line = line?;
            let (module, deps) = line
                .split_once(':')
                .ok_or(Error::new(ErrorKind::Other, "Malformed modules.dep file"))?;

            // This is not the module we are looking for
            if !module.replace("-", "_").contains(&matcher) {
                continue;
            }

            let mut ret = vec![module.to_string()];
            ret.extend(
                deps.split(' ')
                    .filter(|dep| !dep.is_empty())
                    .map(|dep| dep.to_string()),
            );

            return Ok(Some(ret));
        }

        Ok(None)
    }
}

#[cfg(test)]
mod alias_test {
    use super::*;

    fn create_alias_parser() -> AliasParser {
        AliasParser::new(&ModuleDir::new()).expect("The AliasParser should parse successfully")
    }

    /// Tests that modules can be looked up in the Alias Parser via both underscore and
    /// hyphen (as these are largely interchangeable in most implementations).
    #[test]
    fn test_hyphens_and_underscores() {
        let parser = create_alias_parser();

        assert_eq!(
            parser
                .lookup("nft_expr_flow_offload")
                .expect("Alias with underscore should have been found"),
            parser
                .lookup("nft-expr-flow-offload")
                .expect("Alias with hyphen should have been found")
        );
    }

    /// An alias can map to multiple possible modules.
    #[test]
    fn test_multiple() {
        let parser = create_alias_parser();
        let res = parser.lookup("aria").expect("Alias should have been found");
        let expected = vec![
            "aria_aesni_avx_x86_64",
            "aria_aesni_avx2_x86_64",
            "aria_gfni_avx512_x86_64",
            "aria_generic",
        ];

        for item in expected {
            assert!(res.contains(&item.to_string()));
        }
    }

    /// Tests that searching for a module that doesn't exist fails as expected
    #[test]
    fn test_none() {
        let parser = create_alias_parser();
        assert_eq!(None, parser.lookup("non-existant"));
    }
}

#[cfg(test)]
mod deps_test {
    use super::*;

    fn create_parser() -> DepParser {
        DepParser::new(&ModuleDir::new()).expect("The DepParser should open successfully")
    }

    #[test]
    fn test_no_deps() {
        let mut parser = create_parser();
        let ret = parser
            .lookup("leds-pwm")
            .expect("Module should have been parsed")
            .expect("Module should have been found");

        println!("{:?}", ret);
        assert_eq!(1, ret.len());
    }

    #[test]
    fn test_none() {
        let mut parser = create_parser();
        let ret = parser
            .lookup("non-existant")
            .expect("Module should have been parsed");

        println!("{:?}", ret);
        assert_eq!(None, ret);
    }

    #[test]
    fn test_has_deps() {
        let mut parser = create_parser();
        let ret = parser
            .lookup("leds-lm3533")
            .expect("Module should have been parsed")
            .expect("Module should have been found");

        println!("{:?}", ret);
        assert_eq!(
            ret,
            vec![
                "kernel/drivers/leds/leds-lm3533.ko.zst",
                "kernel/drivers/mfd/lm3533-ctrlbank.ko.zst",
                "kernel/drivers/mfd/lm3533-core.ko.zst",
            ]
        );
    }

    /// Tests that modules can be looked up in the DepParser via both underscore and
    /// hyphen (as these are largely interchangeable in most implementations).
    #[test]
    fn test_hyphens_and_underscores() {
        let mut parser = create_parser();

        assert_eq!(
            parser
                .lookup("nfs_acl")
                .expect("Module should have been parsed")
                .expect("Module with underscore should have been found"),
            parser
                .lookup("nfs-acl")
                .expect("Module should have been parsed")
                .expect("Module with hyphen should have been found")
        );
    }
}
