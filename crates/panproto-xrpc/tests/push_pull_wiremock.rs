//! Integration tests driving [`NodeClient::push`] and [`NodeClient::pull`]
//! against a `wiremock` mock node.
//!
//! These exercise the full push/pull pipelines end to end: ref listing,
//! have/want negotiation, object transfer, and ref updates, all against a
//! stubbed `dev.panproto.node.*` XRPC surface. The mock stands in for a
//! real panproto node so the client's HTTP composition (paths, methods,
//! auth headers, msgpack/JSON bodies) is verified without a live server.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use panproto_schema::Protocol;
use panproto_vcs::{MemStore, Object, Store};
use panproto_xrpc::NodeClient;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// XRPC path for an endpoint, e.g. `getObject` -> the full query path.
fn xrpc_path(endpoint: &str) -> String {
    format!("/xrpc/dev.panproto.node.{endpoint}")
}

/// A trivially-constructible content-addressed object used as the payload
/// that crosses the wire in both directions.
fn sample_object() -> Object {
    Object::Protocol(Box::<Protocol>::default())
}

#[tokio::test]
async fn push_transfers_needed_objects_and_updates_refs() {
    let server = MockServer::start().await;

    // A local store holding one object referenced by one branch.
    let mut store = MemStore::new();
    let object = sample_object();
    let oid = store.put(&object).expect("put object");
    store
        .set_ref("refs/heads/main", oid)
        .expect("set local ref");
    let oid_hex = oid.to_string();

    // negotiate: the remote reports it needs exactly our one object.
    Mock::given(method("POST"))
        .and(path(xrpc_path("negotiate")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "need": [oid_hex],
            "refs": [],
        })))
        .expect(1)
        .mount(&server)
        .await;

    // putObject: acknowledge the stored object by echoing its id.
    Mock::given(method("POST"))
        .and(path(xrpc_path("putObject")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "id": oid_hex })))
        .expect(1)
        .mount(&server)
        .await;

    // getRef: the remote has no such ref yet (fast-forward from nothing).
    Mock::given(method("GET"))
        .and(path(xrpc_path("getRef")))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&server)
        .await;

    // setRef: accept the ref update.
    Mock::given(method("POST"))
        .and(path(xrpc_path("setRef")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "ok": true })))
        .expect(1)
        .mount(&server)
        .await;

    let client = NodeClient::new(&server.uri(), "did:plc:test", "demo").with_token("secret-token");

    let result = client.push(&store).await.expect("push succeeds");

    assert_eq!(result.objects_pushed, 1, "one object should be pushed");
    assert_eq!(result.refs_updated, 1, "one ref should be updated");
    // `.expect(1)` on each mock asserts, on server drop, that push issued
    // exactly the expected requests.
}

#[tokio::test]
async fn pull_fetches_needed_objects_and_writes_local_refs() {
    let server = MockServer::start().await;

    // The remote object and the id the client will address it by.
    let object = sample_object();
    let body = rmp_serde::to_vec(&object).expect("encode object");
    // Recompute the id the way the store does, so the ref target matches.
    let oid = {
        let mut probe = MemStore::new();
        probe.put(&object).expect("probe put")
    };
    let oid_hex = oid.to_string();

    // listRefs: the remote advertises one branch pointing at our object.
    Mock::given(method("GET"))
        .and(path(xrpc_path("listRefs")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "refs": [ { "name": "refs/heads/main", "target": oid_hex } ],
        })))
        .expect(1)
        .mount(&server)
        .await;

    // negotiate: the client needs the one object it does not yet have.
    Mock::given(method("POST"))
        .and(path(xrpc_path("negotiate")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "need": [oid_hex],
            "refs": [],
        })))
        .expect(1)
        .mount(&server)
        .await;

    // getObject: return the msgpack-encoded object body.
    Mock::given(method("GET"))
        .and(path(xrpc_path("getObject")))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
        .expect(1)
        .mount(&server)
        .await;

    let client = NodeClient::new(&server.uri(), "did:plc:test", "demo");

    let mut store = MemStore::new();
    let result = client.pull(&mut store).await.expect("pull succeeds");

    assert_eq!(result.objects_fetched, 1, "one object should be fetched");
    assert_eq!(result.refs_updated, 1, "one local ref should be updated");

    // The fetched object and the ref must now live in the local store.
    assert!(store.has(&oid), "pulled object should be stored locally");
    assert_eq!(
        store.get_ref("refs/heads/main").expect("read local ref"),
        Some(oid),
        "local ref should point at the pulled object",
    );
}

#[tokio::test]
async fn a_substituted_object_is_refused() {
    let server = MockServer::start().await;

    // The client asks for one object and the remote answers with a
    // different one. The store is content-addressed, so accepting the
    // answer would file the substituted bytes under an id that does not
    // describe them, and every later reader would trust the wrong id.
    let requested = sample_object();
    let substituted = Object::Protocol(Box::new(Protocol {
        name: "substituted".into(),
        ..Protocol::default()
    }));
    assert_ne!(
        rmp_serde::to_vec(&requested).expect("encode requested"),
        rmp_serde::to_vec(&substituted).expect("encode substituted"),
        "the two objects must differ for this test to mean anything",
    );

    let oid = {
        let mut probe = MemStore::new();
        probe.put(&requested).expect("probe put")
    };
    let oid_hex = oid.to_string();

    Mock::given(method("GET"))
        .and(path(xrpc_path("listRefs")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "refs": [ { "name": "refs/heads/main", "target": oid_hex } ],
        })))
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path(xrpc_path("negotiate")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "need": [oid_hex],
            "refs": [],
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path(xrpc_path("getObject")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(rmp_serde::to_vec(&substituted).expect("encode substituted")),
        )
        .mount(&server)
        .await;

    let client = NodeClient::new(&server.uri(), "did:plc:test", "demo");

    let err = client
        .get_object(&oid)
        .await
        .expect_err("an object that is not the one requested must be refused");
    let rendered = err.to_string();
    assert!(
        rendered.contains(&oid.to_string()),
        "the refusal should name the id that was asked for: {rendered}",
    );

    let mut store = MemStore::new();
    client
        .pull(&mut store)
        .await
        .expect_err("a pull carrying a substituted object must fail");
    assert!(
        !store.has(&oid),
        "a substituted object must not reach the local store",
    );
}
