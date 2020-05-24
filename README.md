# cftokens

This repository hosts the code for building the cftokens binary that underlies the cfformat CommandBox module.

## Syntect

It makes use of the [syntect](https://github.com/trishume/syntect) library along with syntax files from Sublime Text's [Packages](https://github.com/sublimehq/Packages) repository to create an executable that uses the CFML syntax for Sublime Text to generate syntax scopes for component files. You can build the executable yourself by running the `build.cfc` task runner in the root of this repository:

```bash
task run build
```
