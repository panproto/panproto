# Kubernetes CustomResourceDefinitions

Source: <https://github.com/kubernetes/kubernetes>, tag `v1.37.2`, commit
`c50e5395f1ae2d4a2c05f28fdcc38c0916cc1a32` (2026-10-27), licensed under the
Apache License 2.0 (`LICENSE`).

Vendored, unchanged, as `upstream/<file name>`:

- `hack/testdata/CRD/example-crd-1-namespaced.yaml`
- `staging/src/k8s.io/apiextensions-apiserver/pkg/test/example/apiexports_crd.yaml`
- `staging/src/k8s.io/sample-controller/artifacts/examples/crd.yaml`
- `staging/src/k8s.io/sample-controller/artifacts/examples/crd-status-subresource.yaml`
- `test/e2e/testing-manifests/storage-csi/any-volume-datasource/crd/hello-populator-crd.yaml`
- `test/e2e/testing-manifests/storage-csi/any-volume-datasource/crd/populator.storage.k8s.io_volumepopulators.yaml`
- `test/e2e/testing-manifests/storage-csi/external-snapshot-metadata/cbt.storage.k8s.io_snapshotmetadataservices.yaml`
- `cluster/addons/volumesnapshots/crd/snapshot.storage.k8s.io_volumesnapshotclasses.yaml`
- `cluster/addons/volumesnapshots/crd/snapshot.storage.k8s.io_volumesnapshots.yaml`
- `cluster/addons/volumesnapshots/crd/snapshot.storage.k8s.io_volumesnapshotcontents.yaml`
- `cluster/addons/calico-policy-controller/ippool-crd.yaml`

The files under `invalid/` are `example-crd-1-namespaced.yaml` changed to
break one rule each; `MANIFEST` lists the rule and the refusal expected.
`conformance_k8s_crd.rs` runs every case; the cases the reader does not pass
are listed with their limit in `../k8s-crd-limits.txt`.
