# Remote wrong-type subscriptions

The pinned `twoProcPubSub.PubSubWrongTypesOnSubscription` and
`PubRawSubWrongTypesOnSubscription` originals successfully register an Int32
typed callback or an explicitly Int32 raw callback on `/foo`. After the original
1500 ms wait, the corresponding initially false callback flag remains false.
Neither original asserts its callback counter or the ignored callback arguments.

The Rust ports use the existing descriptor-filtered owned subscription API.
A joined observer records any delivered message through ordinary receive or
the owned metadata envelope. It stays live through the fixed observation and a
supplemental matching subscriber's actual Vector3d values 1, 2 and 3. A stop
guard retires the polling observer at the normal tail or assertion unwinding;
cleanup preserves the first failed assertion. The final false flag and native
child success are supplemental checks.

Both ports spawn the exact pinned `twoProcsPublisher_aux` executable. Its
unchanged fixture waits 500 ms, publishes Vector3d and the existing large-topic
message, waits another 1500 ms, publishes both again, then waits 1000 ms and exits.
The 1500 ms observation starts after the owned observer is ready. C++ callback
registration, raw callback storage, exact scheduling and publisher-relative
timing remain unqualified.

The exact two-file source gate passed 225 Rust tests, 24 tooling tests, native
Node 56/0, pub/sub 9/0, ordinary service 6/0 and inputless service 5/0, plus all
other required native groups. Strict coverage remains 2645/2714 production lines
and 3893/4071 regions; no production source changed. All 209 source reports were
retained before metadata or normal-hook work.

Independent [review](reviews/remote-pubsub-wrong-subscription.json) verified all
171 source blobs, replayed both native originals and Rust ports, and checked the
retained full coverage evidence. A private control changed only the wrong
subscription descriptor to Vector3d, with diagnostic payload checks in that
observer. Both observers received values 1, 2 and 3, then both original 1500 ms
false assertions failed. The matching witness alone was not used as the control.
No secondary observer errors occurred. Restoring every source byte and native
pin passed both Rust tests again.

An initial author command replaced the native library path, so both native
children failed to load libzenohc and both traffic-witness checks timed out.
That failed output remains retained separately. The unchanged source passed
after the command preserved the existing native library paths. Final metadata
binding, the mandatory normal protected hook, publication and exact new hosted
success remain required. Broader NET-01/NET-02 and performance remain open.
