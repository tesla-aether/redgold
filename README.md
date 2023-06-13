# <img src="src/resources/svg_rg_2_crop.png" width="9%" height="9%"> Redgold


![Dev](https://github.com/redgold-io/redgold/actions/workflows/ci.yml/badge.svg?branch=dev) | 
[Website](https://redgold.io) |
[Contributing](docs/CONTRIBUTING.md) | [Dev Setup](docs/dev_setup.md) | 
[Whitepaper](docs/whitepaper.md) | [Run A Node](docs/run_a_node.md) | 
[Security Procedures](docs/security_procedures.md)

Redgold or "philosophical gold", is the ideological opposite of BlackRock. It is a decentralized, open-source, and peer-to-peer platform 
designed to act as a financial data and computation layer for the cryptocurrency ecosystem. The primary product 
intention is focused around ETFs & Portfolio target models, and finance, but the platform is designed to be general purpose 
as a decentralized data lake and SQL compute engine. 

It is inspired heavily by Spark and pandas like data transformations on conventional 
parquet data lakes, with the key distinguishing factor being the ability to support multi-tenant compute with 
arbitrary secure UDFs compiled by anyone. WASM executors are used to for secure remote code execution to chain together
transforms operating on SQL-like data loading functions as inputs. Protobuf is used for relational algebra descriptors 
and for raw signature operations. Arrow is used as a cross-memory format for WASM invocations, with sqlite 
tables for frequent access and parquet tables for long-lived data indexes. All operations are translated to work 
with Kademlia distances. [ACCEPT](https://arxiv.org/pdf/2108.05236.pdf) consensus protocol is the most similar 
to the demonstrated primary optimization technique. For a full technical description and motivation of this project 
please refer above to the [whitepaper](docs/whitepaper.md).

## Getting Started



