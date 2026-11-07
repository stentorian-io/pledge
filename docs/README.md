# Documentation

Start with the [quick start](../README.md#quick-start) to install Pledge and
compare a package with its published version. The executable is `pledge`.

## Use Pledge

| I want to… | Read |
| --- | --- |
| Check a package for the first time | [Quick start](../README.md#quick-start) |
| Add a check to CI or configure a private registry | [CI integration](ci.md) |
| Use the self-contained GitHub Action | [GitHub Action](github-action.md) |
| Understand automatic package discovery and baseline selection | [Package selection](package-selection.md) |
| Understand a finding or a failed check | [Reading results](results.md) |
| Consume a report in another tool | [JSON reports](results.md#json-reports) |
| Understand a specific compatibility rule | [Rule reference](rules/README.md) |
| Check language support and known coverage gaps | [JavaScript and TypeScript coverage](language-coverage.md) |
| Review the non-TypeScript coverage boundaries | [ECMAScript](ecmascript-coverage.md), [Test262](test262-coverage.md), [Node](node-coverage.md), [npm](npm-coverage.md) |
| Understand construction or inheritance changes | [Functions and class bases](rules/function-construction.md) |
| Check shipped JSON, schemas, and stylesheets | [Distributed assets](rules/distributed-assets.md) |

## Develop Pledge

| I want to… | Read |
| --- | --- |
| Set up a checkout and contribute a change | [Contributing](../CONTRIBUTING.md) |
| Classify commits, review versions, or release Pledge | [Releasing](releases.md) |
| Find the code responsible for a behavior | [Architecture](architecture.md) |
| Choose a test or add a fixture | [Test architecture](../tests/README.md) |
| Review a published-package regression | [Reference suite](reference-suite.md) |
| Write or update documentation | [Documentation conventions](../CONTRIBUTING.md#documentation) |
