# ovs-unixctl
Library to send commands to OVS daemons though their JSON interface.
See **ovs-appctl(8)**.

## Test

Run unit tests:

```
$ cargo test
```

Run integration tests, if openvswitch is installed in the system:

```
$ cargo test -F test_integration
```
