mod query;

use {
    self::query::AppQueryServer,
    crate::ShielddHost,
    anyhow::Context,
    cnidarium::{
        proto::v1::query_service_server::QueryServiceServer as StorageQueryServiceServer,
        rpc::Server as StorageServer,
    },
    ibc_proto::{
        cosmos::bank::v1beta1::query_server::QueryServer as TransferQueryServer,
        ibc::{
            applications::transfer::v1::query_server::QueryServer as BankQueryServer,
            core::{
                channel::v1::query_server::QueryServer as ChannelQueryServer,
                client::v1::query_server::QueryServer as ClientQueryServer,
                connection::v1::query_server::QueryServer as ConnectionQueryServer,
            },
        },
    },
    shieldd_sdk_compact_block::component::rpc::Server as CompactBlockServer,
    shieldd_sdk_compliance::component::RpcServer as ComplianceServer,
    shieldd_sdk_fee::component::rpc::Server as FeeServer,
    shieldd_sdk_proto::{
        core::{
            app::v1::query_service_server::QueryServiceServer as AppQueryServiceServer,
            component::{
                compact_block::v1::query_service_server::QueryServiceServer as CompactBlockQueryServiceServer,
                compliance::v1::query_service_server::QueryServiceServer as ComplianceQueryServiceServer,
                fee::v1::query_service_server::QueryServiceServer as FeeQueryServiceServer,
                sct::v1::query_service_server::QueryServiceServer as SctQueryServiceServer,
                shielded_pool::v1::query_service_server::QueryServiceServer as ShieldedPoolQueryServiceServer,
            },
        },
        util::{
            node::v1::node_service_server::{NodeService, NodeServiceServer},
            tendermint_proxy::v1::tendermint_proxy_service_server::{
                TendermintProxyService, TendermintProxyServiceServer,
            },
        },
    },
    shieldd_sdk_sct::{
        component::rpc::Server as SctServer, generation_pack::GenerationPackRepository,
    },
    shieldd_sdk_shielded_pool::component::rpc::Server as ShieldedPoolServer,
    tonic::service::{Routes, RoutesBuilder},
    tonic_web::enable as we,
};

fn add_common_routes(
    builder: &mut RoutesBuilder,
    storage: &cnidarium::Storage,
    generation_packs: Option<GenerationPackRepository>,
) -> anyhow::Result<()> {
    let ibc = shieldd_sdk_ibc::component::rpc::IbcQuery::<ShielddHost>::new(storage.clone());

    builder
        // As part of #2932, we are disabling all timeouts until we circle back to our
        // performance story.
        // Sets a timeout for all gRPC requests, but note that in the case of streaming
        // requests, the timeout is only applied to the initial request. This means that
        // this does not prevent long lived streams, for example to allow clients to obtain
        // new blocks.
        // .timeout(std::time::Duration::from_secs(7))
        // Wrap each of the gRPC services in a tonic-web proxy:
        .add_service(we(StorageQueryServiceServer::new(StorageServer::new(
            storage.clone(),
        ))))
        .add_service(we(AppQueryServiceServer::new(AppQueryServer::new(
            storage.clone(),
        ))))
        .add_service(we(CompactBlockQueryServiceServer::new(
            CompactBlockServer::new(storage.clone()),
        )))
        .add_service(we(ComplianceQueryServiceServer::new(
            ComplianceServer::new(storage.clone()),
        )))
        .add_service(we(FeeQueryServiceServer::new(FeeServer::new(
            storage.clone(),
        ))))
        .add_service(we(SctQueryServiceServer::new(match generation_packs {
            Some(repository) => SctServer::with_generation_packs(storage.clone(), repository),
            None => SctServer::new(storage.clone()),
        })))
        .add_service(we(ShieldedPoolQueryServiceServer::new(
            ShieldedPoolServer::new(storage.clone()),
        )))
        .add_service(we(TransferQueryServer::new(ShieldedPoolServer::new(
            storage.clone(),
        ))))
        .add_service(we(BankQueryServer::new(ShieldedPoolServer::new(
            storage.clone(),
        ))))
        .add_service(we(ClientQueryServer::new(ibc.clone())))
        .add_service(we(ChannelQueryServer::new(ibc.clone())))
        .add_service(we(ConnectionQueryServer::new(ibc.clone())))
        .add_service(we(tonic_reflection::server::Builder::configure()
            .register_encoded_file_descriptor_set(shieldd_sdk_proto::FILE_DESCRIPTOR_SET)
            .build_v1()
            .with_context(|| "could not configure grpc reflection service")?));
    Ok(())
}

pub fn routes(
    storage: &cnidarium::Storage,
    tm_proxy: impl TendermintProxyService,
    _enable_expensive_rpc: bool,
) -> anyhow::Result<tonic::service::Routes> {
    let mut builder = Routes::builder();
    add_common_routes(&mut builder, storage, None)?;
    builder.add_service(we(TendermintProxyServiceServer::new(tm_proxy)));
    Ok(builder.routes().prepare())
}

pub fn routes_with_generation_packs(
    storage: &cnidarium::Storage,
    tm_proxy: impl TendermintProxyService,
    _enable_expensive_rpc: bool,
    generation_packs: GenerationPackRepository,
) -> anyhow::Result<tonic::service::Routes> {
    let mut builder = Routes::builder();
    add_common_routes(&mut builder, storage, Some(generation_packs))?;
    builder.add_service(we(TendermintProxyServiceServer::new(tm_proxy)));
    Ok(builder.routes().prepare())
}

pub fn gordian_routes(
    storage: &cnidarium::Storage,
    node_service: impl NodeService,
    _enable_expensive_rpc: bool,
) -> anyhow::Result<tonic::service::Routes> {
    let mut builder = Routes::builder();
    add_common_routes(&mut builder, storage, None)?;
    builder.add_service(we(NodeServiceServer::new(node_service)));
    Ok(builder.routes().prepare())
}
