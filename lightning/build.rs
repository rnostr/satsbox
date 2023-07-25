fn main() -> std::io::Result<()> {
    let lnd = [
        "proto/lnd/signrpc/signer.proto",
        "proto/lnd/walletrpc/walletkit.proto",
        "proto/lnd/lightning.proto",
        "proto/lnd/peersrpc/peers.proto",
        "proto/lnd/verrpc/verrpc.proto",
        "proto/lnd/routerrpc/router.proto",
    ];

    tonic_build::configure()
        .protoc_arg("--experimental_allow_proto3_optional")
        .build_client(true)
        .build_server(false)
        .compile(&lnd, &["proto/lnd"])?;

    let cln = ["proto/cln/node.proto"].to_vec();
    tonic_build::configure()
        .protoc_arg("--experimental_allow_proto3_optional")
        .build_client(true)
        .build_server(false)
        .compile(&cln, &["proto/cln"])?;

    Ok(())
}
