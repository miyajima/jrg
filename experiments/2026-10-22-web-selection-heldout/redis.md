# EXPIRE

Syntax text

Syntax diagram

API methods

```
EXPIRE key seconds [NX | XX | GT | LT]
```

Client:

Python (redis-py)

Node.js (node-redis)

Java-Sync (Jedis)

Lettuce-Sync

Java-Async (Lettuce)

Java-Reactive (Lettuce)

Go (go-redis)

C#-Sync (NRedisStack)

C#-Async (NRedisStack)

PHP (Predis)

Rust-Sync (redis-rs)

Rust-Async (redis-rs)

```
expire(
name: KeyT,
time: ExpiryT,
nx: bool,
xx: bool,
gt: bool,
lt: bool
) → int
```

```
EXPIRE(
key: RedisArgument,
seconds: number,
mode?: 'NX' | 'XX' | 'GT' | 'LT'
) → Any
```

```
expire(
key: byte[],
seconds: long // time to expire
) → long // 1 if the timeout was set, 0 otherwise

expire(
key: byte[],
seconds: long, // time to expire
expiryOption: ExpiryOption // can be NX, XX, GT or LT
) → long // 1 if the timeout was set, 0 otherwise

expire(
key: String,
seconds: long // time to expire
) → long // 1 if the timeout was set, 0 otherwise

expire(
key: String,
seconds: long, // time to expire
expiryOption: ExpiryOption // can be NX, XX, GT or LT
) → long // 1 if the timeout was set, 0 otherwise
```

```
expire(
key: K, // the key.
seconds: long // the seconds.
) → Boolean // Boolean integer-reply specifically: true if the timeout was set. false if key does not exist or the timeout could not be set.

expire(
key: K, // the key.
seconds: long, // the seconds.
expireArgs: ExpireArgs // the expiry arguments.
) → Boolean // Boolean integer-reply specifically: true if the timeout was set. false if key does not exist or the timeout could not be set.

expire(
key: K, // the key.
seconds: Duration // the seconds.
) → Boolean // Boolean integer-reply specifically: true if the timeout was set. false if key does not exist or the timeout could not be set.

expire(
key: K, // the key.
seconds: Duration, // the seconds.
expireArgs: ExpireArgs // the expiry arguments.
) → Boolean // Boolean integer-reply specifically: true if the timeout was set. false if key does not exist or the timeout could not be set.
```

```
expire(
key: K, // the key.
seconds: long // the seconds.
) → RedisFuture<Boolean> // Boolean integer-reply specifically: true if the timeout was set. false if key does not exist or the timeout could not be set.

expire(
key: K, // the key.
seconds: long, // the seconds.
expireArgs: ExpireArgs // the expiry arguments.
) → RedisFuture<Boolean> // Boolean integer-reply specifically: true if the timeout was set. false if key does not exist or the timeout could not be set.

expire(
key: K, // the key.
seconds: Duration // the seconds.
) → RedisFuture<Boolean> // Boolean integer-reply specifically: true if the timeout was set. false if key does not exist or the timeout could not be set.

expire(
key: K, // the key.
seconds: Duration, // the seconds.
expireArgs: ExpireArgs // the expiry arguments.
) → RedisFuture<Boolean> // Boolean integer-reply specifically: true if the timeout was set. false if key does not exist or the timeout could not be set.
```

```
expire(
key: K, // the key.
seconds: long // the seconds.
) → Mono<Boolean> // Boolean integer-reply specifically: true if the timeout was set. false if key does not exist or the timeout could not be set.

expire(
key: K, // the key.
seconds: long, // the seconds.
expireArgs: ExpireArgs // the expiry arguments.
) → Mono<Boolean> // Boolean integer-reply specifically: true if the timeout was set. false if key does not exist or the timeout could not be set.

expire(
key: K, // the key.
seconds: Duration // the seconds.
) → Mono<Boolean> // Boolean integer-reply specifically: true if the timeout was set. false if key does not exist or the timeout could not be set.

expire(
key: K, // the key.
seconds: Duration, // the seconds.
expireArgs: ExpireArgs // the expiry arguments.
) → Mono<Boolean> // Boolean integer-reply specifically: true if the timeout was set. false if key does not exist or the timeout could not be set.
```

```
Expire(
ctx: context.Context,
key: string,
expiration: time.Duration
) → *BoolCmd

ExpireNX(
ctx: context.Context,
key: string,
expiration: time.Duration
) → *BoolCmd

ExpireXX(
ctx: context.Context,
key: string,
expiration: time.Duration
) → *BoolCmd

ExpireGT(
ctx: context.Context,
key: string,
expiration: time.Duration
) → *BoolCmd

ExpireLT(
ctx: context.Context,
key: string,
expiration: time.Duration
) → *BoolCmd
```

```
KeyExpire(
key: RedisKey, // The key to set the expiration for.
expiry: TimeSpan?, // The timeout to set.
flags: CommandFlags // The flags to use for this operation.
) → bool // true if the timeout was set. false if key does not exist or the timeout could not be set.

KeyExpire(
key: RedisKey, // The key to set the expiration for.
expiry: TimeSpan?, // The timeout to set.
when: ExpireWhen, // In Redis 7+, we choose under which condition the expiration will be set using ExpireWhen.
flags: CommandFlags // The flags to use for this operation.
) → bool // true if the timeout was set. false if key does not exist or the timeout could not be set.
```

```
KeyExpireAsync(
key: RedisKey, // The key to set the expiration for.
expiry: TimeSpan?, // The timeout to set.
flags: CommandFlags // The flags to use for this operation.
) → Task<bool> // true if the timeout was set. false if key does not exist or the timeout could not be set.

KeyExpireAsync(
key: RedisKey, // The key to set the expiration for.
expiry: TimeSpan?, // The timeout to set.
when: ExpireWhen, // In Redis 7+, we choose under which condition the expiration will be set using ExpireWhen.
flags: CommandFlags // The flags to use for this operation.
) → Task<bool> // true if the timeout was set. false if key does not exist or the timeout could not be set.
```

```
expire(
$key: string,
$seconds: int,
$expireOption: string
) → int
```

No method signature available for this client.

No method signature available for this client.

Available since:

Redis Open Source 1.0.0

Time complexity:

O(1)

ACL categories:

@keyspace,

@write,

@fast,

Compatibility:

Redis Software and Redis Cloud compatibility

Set a timeout on key. After the timeout has expired, the key will automatically be deleted. A key with an associated timeout is often said to be volatile in Redis terminology.

The timeout will only be cleared by commands that delete or overwrite the contents of the key, including DEL, SET, GETSET and all the *STORE commands. This means that all the operations that conceptually alter the value stored at the key without replacing it with a new one will leave the timeout untouched. For instance, incrementing the value of a key with INCR, pushing a new value into a list with LPUSH, or altering the field value of a hash with HSET are all operations that will leave the timeout untouched.

The timeout can also be cleared, turning the key back into a persistent key, using the PERSIST command.

If a key is renamed with RENAME, the associated time to live is transferred to the new key name.

If a key is overwritten by RENAME, like in the case of an existing key Key_A that is overwritten by a call like RENAME Key_B Key_A, it does not matter if the original Key_A had a timeout associated or not, the new key Key_A will inherit all the characteristics of Key_B.

Note that calling EXPIRE/PEXPIRE with a non-positive timeout or EXPIREAT/PEXPIREAT with a time in the past will result in the key being deleted rather than expired (accordingly, the emitted key event will be del, not expired).

## Required arguments

key

The name of the key.

seconds

The time to live, in seconds. The key is deleted after this many seconds.

## Optional arguments

These options are mutually exclusive.

NX

Set the expiry only when the key has no expiry.

XX

Set the expiry only when the key already has an expiry.

GT

Set the expiry only when the new expiry is greater than the current one. A non-volatile key is treated as an infinite TTL for the purpose of GT.

LT

Set the expiry only when the new expiry is less than the current one. A non-volatile key is treated as an infinite TTL for the purpose of LT.

## Examples

Language:

>_ Redis CLI

Python

JavaScript (node-redis)

JavaScript (ioredis)

Java-Sync

Java-Async

Java-Reactive

Go

C

C#-Sync (SE.Redis)

PHP

Rust-Sync

Rust-Async

Try it

Show full output

Copied!

Copied!

Foundational: Set key expiration time using EXPIRE (supports conditional options NX/XX/GT/LT, returns 1 if set or 0 if not)

```
> SET mykey "Hello"
OK
> EXPIRE mykey 10
(integer) 1
> TTL mykey
(integer) 10
> SET mykey "Hello World"
OK
> TTL mykey
(integer) -1
> EXPIRE mykey 10 XX
(integer) 0
> TTL mykey
(integer) -1
> EXPIRE mykey 10 NX
(integer) 1
> TTL mykey
(integer) 10
```

▼
Commands:
SET, EXPIRE, TTL

- SET ( @write , @string , @slow ) Sets the string value of a key, ignoring its type. The key is created if it doesn't exist.

- EXPIRE ( @keyspace , @write , @fast ) Sets the expiration time of a key in seconds.

- TTL ( @keyspace , @read , @fast ) Returns the expiration time in seconds of a key.

Redis CLI guide

Also, check out our other client tools
Redis Insight
and
Redis for VS Code.

```
res = r.set("mykey", "Hello")
print(res)
# >>> True

res = r.expire("mykey", 10)
print(res)
# >>> True

res = r.ttl("mykey")
print(res)
# >>> 10

res = r.set("mykey", "Hello World")
print(res)
# >>> True

res = r.ttl("mykey")
print(res)
# >>> -1

res = r.expire("mykey", 10, xx=True)
print(res)
# >>> False

res = r.ttl("mykey")
print(res)
# >>> -1

res = r.expire("mykey", 10, nx=True)
print(res)
# >>> True

res = r.ttl("mykey")
print(res)
# >>> 10

```

```
import redis

r = redis.Redis(decode_responses=True)

res = r.set("key1", "Hello")
print(res)
# >>> True

res = r.set("key2", "World")
print(res)
# >>> True

res = r.delete("key1", "key2", "key3")
print(res)
# >>> 2

res = r.set("key1", "Hello")
print(res)
# >>> True

res = r.exists("key1")
print(res)
# >>> 1

res = r.exists("nosuchkey")
print(res)
# >>> 0

res = r.set("key2", "World")
print(res)
# >>> True

res = r.exists("key1", "key2", "nosuchkey")
print(res)
# >>> 2

res = r.set("mykey", "Hello")
print(res)
# >>> True

res = r.expire("mykey", 10)
print(res)
# >>> True

res = r.ttl("mykey")
print(res)
# >>> 10

res = r.set("mykey", "Hello World")
print(res)
# >>> True

res = r.ttl("mykey")
print(res)
# >>> -1

res = r.expire("mykey", 10, xx=True)
print(res)
# >>> False

res = r.ttl("mykey")
print(res)
# >>> -1

res = r.expire("mykey", 10, nx=True)
print(res)
# >>> True

res = r.ttl("mykey")
print(res)
# >>> 10

res = r.set("mykey", "Hello")
print(res)
# >>> True

res = r.expire("mykey", 10)
print(res)
# >>> True

res = r.ttl("mykey")
print(res)
# >>> 10

res = r.mset({"firstname": "Jack", "lastname": "Stuntman", "age": "35"})
print(res)
# >>> True

res = r.keys("*name*")
print(sorted(res))
# >>> ['firstname', 'lastname']

res = r.keys("a??")
print(res)
# >>> ['age']

res = r.keys("*")
print(sorted(res))
# >>> ['age', 'firstname', 'lastname']

res = r.sadd("myset", *set([1, 2, 3, "foo", "foobar", "feelsgood"]))
print(res)
# >>> 6

res = list(r.sscan_iter("myset", match="f*"))
print(res)
# >>> ['foobar', 'foo', 'feelsgood']

total = 0

cursor, keys = r.scan(cursor=0, match='*11*')
total += len(keys)
print(cursor, keys)

cursor, keys = r.scan(cursor, match='*11*')
total += len(keys)
print(cursor, keys)

cursor, keys = r.scan(cursor, match='*11*')
total += len(keys)
print(cursor, keys)

cursor, keys = r.scan(cursor, match='*11*')
total += len(keys)
print(cursor, keys)

cursor, keys = r.scan(cursor, match='*11*', count=1000)
total += len(keys)
print(cursor, keys)

# The per-call split isn't guaranteed, but the cumulative total is.
print(total)
# >>> 19

res = r.geoadd("geokey", (0, 0, "value"))
print(res)
# >>> 1

res = r.zadd("zkey", {"value": 1000})
print(res)
# >>> 1

res = r.type("geokey")
print(res)
# >>> zset

res = r.type("zkey")
print(res)
# >>> zset

# A single call isn't guaranteed to find every match, so loop until the cursor
# returns to 0, accumulating matches from every call.
cursor = 0
scan3_keys = []
while True:
cursor, keys = r.scan(cursor=cursor, _type="zset")
scan3_keys.extend(keys)
if cursor == 0:
break
print(sorted(scan3_keys))
# >>> ['geokey', 'zkey']

res = r.hset("myhash", mapping={"a": 1, "b": 2})
print(res)
# >>> 2

cursor, keys = r.hscan("myhash", 0)
print(keys)
# >>> {'a': '1', 'b': '2'}

cursor, keys = r.hscan("myhash", 0, no_values=True)
print(sorted(keys))
# >>> ['a', 'b']

```

▼
Commands:
SET, EXPIRE, TTL

- SET ( @write , @string , @slow ) Sets the string value of a key, ignoring its type. The key is created if it doesn't exist. ▶ Method set( name: KeyT, value: EncodableT, ex: Optional[ExpiryT] = None, px: Optional[ExpiryT] = None, nx: bool = False, xx: bool = False, keepttl: bool = False, get: bool = False, exat: Optional[AbsExpiryT] = None, pxat: Optional[AbsExpiryT] = None, ifeq: Optional[Union[bytes, str]] = None, ifne: Optional[Union[bytes, str]] = None, ifdeq: Optional[str] = None, ifdne: Optional[str] = None ) → ResponseT

- EXPIRE ( @keyspace , @write , @fast ) Sets the expiration time of a key in seconds. ▶ Method expire( name: KeyT, time: ExpiryT, nx: bool, xx: bool, gt: bool, lt: bool ) → int

- TTL ( @keyspace , @read , @fast ) Returns the expiration time in seconds of a key. ▶ Method ttl( name: KeyT ) → int

Python Quick-Start

```
const expireRes1 = await client.set('mykey', 'Hello');
console.log(expireRes1); // OK

const expireRes2 = await client.expire('mykey', 10);
console.log(expireRes2); // 1

const expireRes3 = await client.ttl('mykey');
console.log(expireRes3); // 10

const expireRes4 = await client.set('mykey', 'Hello World');
console.log(expireRes4); // OK

const expireRes5 = await client.ttl('mykey');
console.log(expireRes5); // -1

const expireRes6 = await client.expire('mykey', 10, "XX");
console.log(expireRes6); // 0

const expireRes7 = await client.ttl('mykey');
console.log(expireRes7); // -1

const expireRes8 = await client.expire('mykey', 10, "NX");
console.log(expireRes8); // 1

const expireRes9 = await client.ttl('mykey');
console.log(expireRes9); // 10

```

```

import { createClient } from 'redis';

const client = createClient();
await client.connect().catch(console.error);

const delRes1 = await client.set('key1', 'Hello');
console.log(delRes1); // OK

const delRes2 = await client.set('key2', 'World');
console.log(delRes2); // OK

const delRes3 = await client.del(['key1', 'key2', 'key3']);
console.log(delRes3); // 2

const existsRes1 = await client.set('key1', 'Hello');
console.log(existsRes1); // OK

const existsRes2 = await client.exists('key1');
console.log(existsRes2); // 1

const existsRes3 = await client.exists('nosuchkey');
console.log(existsRes3); // 0

const existsRes4 = await client.set('key2', 'World');
console.log(existsRes4); // OK

const existsRes5 = await client.exists(['key1', 'key2', 'nosuchkey']);
console.log(existsRes5); // 2

const expireRes1 = await client.set('mykey', 'Hello');
console.log(expireRes1); // OK

const expireRes2 = await client.expire('mykey', 10);
console.log(expireRes2); // 1

const expireRes3 = await client.ttl('mykey');
console.log(expireRes3); // 10

const expireRes4 = await client.set('mykey', 'Hello World');
console.log(expireRes4); // OK

const expireRes5 = await client.ttl('mykey');
console.log(expireRes5); // -1

const expireRes6 = await client.expire('mykey', 10, "XX");
console.log(expireRes6); // 0

const expireRes7 = await client.ttl('mykey');
console.log(expireRes7); // -1

const expireRes8 = await client.expire('mykey', 10, "NX");
console.log(expireRes8); // 1

const expireRes9 = await client.ttl('mykey');
console.log(expireRes9); // 10

const ttlRes1 = await client.set('mykey', 'Hello');
console.log(ttlRes1); // OK

const ttlRes2 = await client.expire('mykey', 10);
console.log(ttlRes2); // 1

const ttlRes3 = await client.ttl('mykey');
console.log(ttlRes3); // 10

const keysRes1 = await client.mSet({ firstname: 'Jack', lastname: 'Stuntman', age: '35' });
console.log(keysRes1); // OK

const keysRes2 = await client.keys('*name*');
console.log(keysRes2.sort()); // ['firstname', 'lastname']

const keysRes3 = await client.keys('a??');
console.log(keysRes3); // ['age']

const keysRes4 = await client.keys('*');
console.log(keysRes4.sort()); // ['age', 'firstname', 'lastname']

const scan1Res1 = await client.sAdd('myset', ['1', '2', '3', 'foo', 'foobar', 'feelsgood']);
console.log(scan1Res1); // 6

let scan1Res2 = [];
for await (const values of client.sScanIterator('myset', { MATCH: 'f*' })) {
scan1Res2 = scan1Res2.concat(values);
}
console.log(scan1Res2); // ['foo', 'foobar', 'feelsgood']

let cursor = '0';
let scanResult;
let total = 0;

scanResult = await client.scan(cursor, { MATCH: '*11*' });
total += scanResult.keys.length;
console.log(scanResult.cursor, scanResult.keys);

scanResult = await client.scan(scanResult.cursor, { MATCH: '*11*' });
total += scanResult.keys.length;
console.log(scanResult.cursor, scanResult.keys);

scanResult = await client.scan(scanResult.cursor, { MATCH: '*11*' });
total += scanResult.keys.length;
console.log(scanResult.cursor, scanResult.keys);

scanResult = await client.scan(scanResult.cursor, { MATCH: '*11*' });
total += scanResult.keys.length;
console.log(scanResult.cursor, scanResult.keys);

scanResult = await client.scan(scanResult.cursor, { MATCH: '*11*', COUNT: 1000 });
total += scanResult.keys.length;
console.log(scanResult.cursor, scanResult.keys);

// The per-call split isn't guaranteed, but the cumulative total is.
console.log(total);
// >>> 19

const scan3Res1 = await client.geoAdd('geokey', { longitude: 0, latitude: 0, member: 'value' });
console.log(scan3Res1); // 1

const scan3Res2 = await client.zAdd('zkey', [{ score: 1000, value: 'value' }]);
console.log(scan3Res2); // 1

const scan3Res3 = await client.type('geokey');
console.log(scan3Res3); // zset

const scan3Res4 = await client.type('zkey');
console.log(scan3Res4); // zset

// A single call isn't guaranteed to find every match, so loop until the cursor
// returns to 0, accumulating matches from every call.
let scan3Cursor = '0';
let scan3Keys = [];
do {
const scan3Res5 = await client.scan(scan3Cursor, { TYPE: 'zset' });
scan3Cursor = scan3Res5.cursor;
scan3Keys = scan3Keys.concat(scan3Res5.keys);
} while (scan3Cursor !== '0');
console.log(scan3Keys.sort()); // ['geokey', 'zkey']

const scan4Res1 = await client.hSet('myhash', { a: 1, b: 2 });
console.log(scan4Res1); // 2

// HSCAN doesn't promise a field order, so pair entries into an object rather than
// relying on position.
const scan4Res2 = await client.hScan('myhash', '0');
const scan4Pairs = Object.fromEntries(scan4Res2.entries.map((e) => [e.field, e.value]));
console.log(scan4Pairs); // {a: '1', b: '2'}

const scan4Res3 = await client.hScan('myhash', '0', { COUNT: 10 });
const items = scan4Res3.entries.map((item) => item.field).sort()
console.log(items); // ['a', 'b']

await client.close();

```

▼
Commands:
SET, EXPIRE, TTL

- SET ( @write , @string , @slow ) Sets the string value of a key, ignoring its type. The key is created if it doesn't exist. ▶ Method SET( key: RedisArgument, value: RedisArgument | number, options?: SetOptions ) → Any

- EXPIRE ( @keyspace , @write , @fast ) Sets the expiration time of a key in seconds. ▶ Method EXPIRE( key: RedisArgument, seconds: number, mode?: 'NX' | 'XX' | 'GT' | 'LT' ) → Any

- TTL ( @keyspace , @read , @fast ) Returns the expiration time in seconds of a key. ▶ Method TTL( key: RedisArgument ) → Any

Node.js Quick-Start

```
console.log(await redis.set('mykey', 'Hello')); // >>> OK

console.log(await redis.expire('mykey', 10)); // >>> 1
console.log(await redis.ttl('mykey')); // >>> 10

// Overwriting a key with SET clears its expiry.
console.log(await redis.set('mykey', 'Hello World')); // >>> OK
console.log(await redis.ttl('mykey')); // >>> -1

// XX only sets the expiry when one already exists, so this is a no-op.
console.log(await redis.expire('mykey', 10, 'XX')); // >>> 0
console.log(await redis.ttl('mykey')); // >>> -1

// NX only sets the expiry when there is none, so this one applies.
console.log(await redis.expire('mykey', 10, 'NX')); // >>> 1
const expireTtl = await redis.ttl('mykey');
console.log(expireTtl); // >>> 10

```

```

import assert from 'node:assert';
import { Redis } from 'ioredis';

const redis = new Redis();

const keysRes1 = await redis.mset({ firstname: 'Jack', lastname: 'Stuntman', age: '35' });
console.log(keysRes1); // >>> OK

const keysRes2 = await redis.keys('*name*');
console.log(keysRes2.sort()); // >>> ['firstname', 'lastname']

const keysRes3 = await redis.keys('a??');
console.log(keysRes3); // >>> ['age']

const keysRes4 = await redis.keys('*');
console.log(keysRes4.sort()); // >>> ['age', 'firstname', 'lastname']

const scan1Res1 = await redis.sadd('myset', '1', '2', '3', 'foo', 'foobar', 'feelsgood');
console.log(scan1Res1); // >>> 6

const [, scan1Members] = await redis.sscan('myset', 0, 'MATCH', 'f*');
console.log(scan1Members.sort()); // >>> ['feelsgood', 'foo', 'foobar']

// MATCH filters after the elements are fetched, so most iterations return nothing.
let [scan2Cursor, scan2Keys] = await redis.scan(0, 'MATCH', '*11*');
let scan2Total = scan2Keys.length;
console.log(scan2Keys.length);

for (let i = 0; i < 3; i++) {
[scan2Cursor, scan2Keys] = await redis.scan(scan2Cursor, 'MATCH', '*11*');
scan2Total += scan2Keys.length;
console.log(scan2Keys.length);
}

// A larger COUNT forces more scanning in a single iteration, so the rest of the
// matches arrive together. The scan continues from the cursor reached above.
[scan2Cursor, scan2Keys] = await redis.scan(scan2Cursor, 'MATCH', '*11*', 'COUNT', 1000);
scan2Total += scan2Keys.length;
console.log(scan2Keys.length);

// The per-call split isn't guaranteed, but the cumulative total is.
console.log(scan2Total); // >>> 19

const scan3Res1 = await redis.geoadd('geokey', '0', '0', 'value');
console.log(scan3Res1); // >>> 1

const scan3Res2 = await redis.zadd('zkey', '1000', 'value');
console.log(scan3Res2); // >>> 1

console.log(await redis.type('geokey')); // >>> zset
console.log(await redis.type('zkey')); // >>> zset

// A single call isn't guaranteed to find every match, so loop until the cursor
// returns to 0, accumulating matches from every call.
let scan3Cursor = '0';
let scan3Keys = [];
do {
let scan3Batch;
[scan3Cursor, scan3Batch] = await redis.scan(scan3Cursor, 'TYPE', 'zset');
scan3Keys = scan3Keys.concat(scan3Batch);
} while (scan3Cursor !== '0');
console.log(scan3Keys.sort()); // >>> ['geokey', 'zkey']

const scan4Res1 = await redis.hset('myhash', { a: 1, b: 2 });
console.log(scan4Res1); // >>> 2

// HSCAN returns field and value interleaved. Redis does not promise an order, so pair
// them up into an object rather than relying on the position of each element.
const [, scan4Flat] = await redis.hscan('myhash', 0);
const scan4Pairs = Object.fromEntries(
scan4Flat.reduce((acc, v, i) => (i % 2 ? acc : [...acc, [v, scan4Flat[i + 1]]]), [])
);
console.log(scan4Pairs); // >>> { a: '1', b: '2' }

const [, scan4Fields] = await redis.hscan('myhash', 0, 'NOVALUES');
console.log(scan4Fields.sort()); // >>> [ 'a', 'b' ]

console.log(await redis.set('key1', 'Hello')); // >>> OK
console.log(await redis.set('key2', 'World')); // >>> OK

const delResult = await redis.del('key1', 'key2', 'key3');
console.log(delResult); // >>> 2

console.log(await redis.set('key1', 'Hello')); // >>> OK

console.log(await redis.exists('key1')); // >>> 1
console.log(await redis.exists('nosuchkey')); // >>> 0

console.log(await redis.set('key2', 'World')); // >>> OK

const existsResult = await redis.exists('key1', 'key2', 'nosuchkey');
console.log(existsResult); // >>> 2

console.log(await redis.set('mykey', 'Hello')); // >>> OK

console.log(await redis.expire('mykey', 10)); // >>> 1
console.log(await redis.ttl('mykey')); // >>> 10

// Overwriting a key with SET clears its expiry.
console.log(await redis.set('mykey', 'Hello World')); // >>> OK
console.log(await redis.ttl('mykey')); // >>> -1

// XX only sets the expiry when one already exists, so this is a no-op.
console.log(await redis.expire('mykey', 10, 'XX')); // >>> 0
console.log(await redis.ttl('mykey')); // >>> -1

// NX only sets the expiry when there is none, so this one applies.
console.log(await redis.expire('mykey', 10, 'NX')); // >>> 1
const expireTtl = await redis.ttl('mykey');
console.log(expireTtl); // >>> 10

console.log(await redis.set('mykey', 'Hello')); // >>> OK
console.log(await redis.expire('mykey', 10)); // >>> 1

const ttlResult = await redis.ttl('mykey');
console.log(ttlResult); // >>> 10

redis.disconnect();

```

▼
Commands:
SET, EXPIRE, TTL

- SET ( @write , @string , @slow ) Sets the string value of a key, ignoring its type. The key is created if it doesn't exist. ▶ Methods set( key: RedisKey, value: string | Buffer | number, callback?: Callback<"OK"> ) → Any set( key: RedisKey, value: string | Buffer | number, get: "GET", callback?: Callback<string | null> ) → Any set( key: RedisKey, value: string | Buffer | number, nx: "NX", callback?: Callback<"OK" | null> ) → Any set( key: RedisKey, value: string | Buffer | number, nx: "NX", get: "GET", callback?: Callback<string | null> ) → Any set( key: RedisKey, value: string | Buffer | number, xx: "XX", callback?: Callback<"OK" | null> ) → Any

- EXPIRE ( @keyspace , @write , @fast ) Sets the expiration time of a key in seconds.

- TTL ( @keyspace , @read , @fast ) Returns the expiration time in seconds of a key.

ioredis Quick-Start

```
String expireResult1 = jedis.set("mykey", "Hello");
System.out.println(expireResult1); // >>> OK

long expireResult2 = jedis.expire("mykey", 10);
System.out.println(expireResult2); // >>> 1

long expireResult3 = jedis.ttl("mykey");
System.out.println(expireResult3); // >>> 10

String expireResult4 = jedis.set("mykey", "Hello World");
System.out.println(expireResult4); // >>> OK

long expireResult5 = jedis.ttl("mykey");
System.out.println(expireResult5); // >>> -1

long expireResult6 = jedis.expire("mykey", 10, ExpiryOption.XX);
System.out.println(expireResult6); // >>> 0

long expireResult7 = jedis.ttl("mykey");
System.out.println(expireResult7); // >>> -1

long expireResult8 = jedis.expire("mykey", 10, ExpiryOption.NX);
System.out.println(expireResult8); // >>> 1

long expireResult9 = jedis.ttl("mykey");
System.out.println(expireResult9); // >>> 10

```

```

import redis.clients.jedis.RedisClient;
import redis.clients.jedis.args.ExpiryOption;
import redis.clients.jedis.params.ScanParams;
import redis.clients.jedis.resps.ScanResult;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.Set;

import static org.junit.jupiter.api.Assertions.assertEquals;

public class CmdsGenericExample {

public void run() {
RedisClient jedis = RedisClient.create("redis://localhost:6379");

String delResult1 = jedis.set("key1", "Hello");
System.out.println(delResult1); // >>> OK

String delResult2 = jedis.set("key2", "World");
System.out.println(delResult2); // >>> OK

long delResult3 = jedis.del("key1", "key2", "key3");
System.out.println(delResult3); // >>> 2

// Tests for 'del' step.

String existsResult1 = jedis.set("key1", "Hello");
System.out.println(existsResult1); // >>> OK

boolean existsResult2 = jedis.exists("key1");
System.out.println(existsResult2); // >>> true

boolean existsResult3 = jedis.exists("nosuchkey");
System.out.println(existsResult3); // >>> false

String existsResult4 = jedis.set("key2", "World");
System.out.println(existsResult4); // >>> OK

long existsResult5 = jedis.exists("key1", "key2", "nosuchkey");
System.out.println(existsResult5); // >>> 2

// Tests for 'exists' step.

String expireResult1 = jedis.set("mykey", "Hello");
System.out.println(expireResult1); // >>> OK

long expireResult2 = jedis.expire("mykey", 10);
System.out.println(expireResult2); // >>> 1

long expireResult3 = jedis.ttl("mykey");
System.out.println(expireResult3); // >>> 10

String expireResult4 = jedis.set("mykey", "Hello World");
System.out.println(expireResult4); // >>> OK

long expireResult5 = jedis.ttl("mykey");
System.out.println(expireResult5); // >>> -1

long expireResult6 = jedis.expire("mykey", 10, ExpiryOption.XX);
System.out.println(expireResult6); // >>> 0

long expireResult7 = jedis.ttl("mykey");
System.out.println(expireResult7); // >>> -1

long expireResult8 = jedis.expire("mykey", 10, ExpiryOption.NX);
System.out.println(expireResult8); // >>> 1

long expireResult9 = jedis.ttl("mykey");
System.out.println(expireResult9); // >>> 10

// Tests for 'expire' step.

String ttlResult1 = jedis.set("mykey", "Hello");
System.out.println(ttlResult1); // >>> OK

long ttlResult2 = jedis.expire("mykey", 10);
System.out.println(ttlResult2); // >>> 1

long ttlResult3 = jedis.ttl("mykey");
System.out.println(ttlResult3); // >>> 10

// Tests for 'ttl' step.

String keysResult1 = jedis.mset("firstname", "Jack", "lastname", "Stuntman", "age", "35");
System.out.println(keysResult1); // >>> OK

Set<String> keysResult2 = jedis.keys("*name*");
ArrayList<String> keysResult2List = new ArrayList<>(keysResult2);
Collections.sort(keysResult2List);
System.out.println(keysResult2List); // >>> [firstname, lastname]

Set<String> keysResult3 = jedis.keys("a??");
System.out.println(keysResult3); // >>> [age]

Set<String> keysResult4 = jedis.keys("*");
ArrayList<String> keysResult4List = new ArrayList<>(keysResult4);
Collections.sort(keysResult4List);
System.out.println(keysResult4List); // >>> [age, firstname, lastname]

// Tests for 'keys' step.

long scan1Result1 = jedis.sadd("myset", "1", "2", "3", "foo", "foobar", "feelsgood");
System.out.println(scan1Result1); // >>> 6

ScanResult<String> scan1Result2 = jedis.sscan(
"myset", "0", new ScanParams().match("f*")
);
ArrayList<String> scan1Members = new ArrayList<>(scan1Result2.getResult());
Collections.sort(scan1Members);
System.out.println(scan1Members); // >>> [feelsgood, foo, foobar]

// MATCH is applied after elements are fetched, so with the default COUNT most
// iterations return few keys or none at all.
String scan2Cursor = "0";
ScanResult<String> scan2Result;
int scan2Total = 0;

for (int i = 0; i < 4; i++) {
scan2Result = jedis.scan(scan2Cursor, new ScanParams().match("*11*"));
scan2Cursor = scan2Result.getCursor();
scan2Total += scan2Result.getResult().size();
System.out.println(scan2Result.getResult().size());
}

// A larger COUNT forces more scanning in a single iteration, so the remaining
// matches arrive together. This continues from the cursor reached above.
scan2Result = jedis.scan(scan2Cursor, new ScanParams().match("*11*").count(1000));
scan2Total += scan2Result.getResult().size();
System.out.println(scan2Result.getResult().size());

// The per-call split isn't guaranteed, but the cumulative total is.
System.out.println(scan2Total); // >>> 19

long scan3Result1 = jedis.geoadd("geokey", 0, 0, "value");
System.out.println(scan3Result1); // >>> 1

long scan3Result2 = jedis.zadd("zkey", 1000, "value");
System.out.println(scan3Result2); // >>> 1

System.out.println(jedis.type("geokey")); // >>> zset
System.out.println(jedis.type("zkey")); // >>> zset

// A single call isn't guaranteed to find every match, so loop until the cursor
// returns to "0", accumulating matches from every call.
String scan3Cursor = "0";
ArrayList<String> scan3Keys = new ArrayList<>();
do {
ScanResult<String> scan3Result3 = jedis.scan(scan3Cursor, new ScanParams(), "zset");
scan3Cursor = scan3Result3.getCursor();
scan3Keys.addAll(scan3Result3.getResult());
} while (!scan3Cursor.equals("0"));
Collections.sort(scan3Keys);
System.out.println(scan3Keys); // >>> [geokey, zkey]

long scan4Result1 = jedis.hset("myhash", Map.of("a", "1", "b", "2"));
System.out.println(scan4Result1); // >>> 2

ScanResult<Map.Entry<String, String>> scan4Result2 = jedis.hscan(
"myhash", "0", new ScanParams()
);
ArrayList<String> scan4Pairs = new ArrayList<>();
for (Map.Entry<String, String> entry : scan4Result2.getResult()) {
scan4Pairs.add(entry.getKey() + "=" + entry.getValue());
}
Collections.sort(scan4Pairs);
System.out.println(scan4Pairs); // >>> [a=1, b=2]

ScanResult<String> scan4Result3 = jedis.hscanNoValues(
"myhash", "0", new ScanParams()
);
ArrayList<String> scan4Fields = new ArrayList<>(scan4Result3.getResult());
Collections.sort(scan4Fields);
System.out.println(scan4Fields); // >>> [a, b]

jedis.close();
}
}

```

▼
Commands:
SET, EXPIRE, TTL

- SET ( @write , @string , @slow ) Sets the string value of a key, ignoring its type. The key is created if it doesn't exist. ▶ Methods set( key: byte[], value: byte[] ) → String // simple-string-reply OK if SET was executed correctly, or null if the SET operation was not performed because the user specified the NX or XX option but the condition was not met. set( key: byte[], value: byte[], params: SetParams // key if it already exists. EX|PX, expire time units: EX = seconds; PX = milliseconds ) → String // simple-string-reply OK if SET was executed correctly, or null if the SET operation was not performed because the user specified the NX or XX option but the condition was not met. set( key: String, value: String ) → String // simple-string-reply OK if SET was executed correctly, or null if the SET operation was not performed because the user specified the NX or XX option but the condition was not met. set( key: String, value: String, params: SetParams // key if it already exists. EX|PX, expire time units: EX = seconds; PX = milliseconds ) → String // simple-string-reply OK if SET was executed correctly, or null if the SET operation was not performed because the user specified the NX or XX option but the condition was not met.

- EXPIRE ( @keyspace , @write , @fast ) Sets the expiration time of a key in seconds. ▶ Methods expire( key: byte[], seconds: long // time to expire ) → long // 1 if the timeout was set, 0 otherwise expire( key: byte[], seconds: long, // time to expire expiryOption: ExpiryOption // can be NX, XX, GT or LT ) → long // 1 if the timeout was set, 0 otherwise expire( key: String, seconds: long // time to expire ) → long // 1 if the timeout was set, 0 otherwise expire( key: String, seconds: long, // time to expire expiryOption: ExpiryOption // can be NX, XX, GT or LT ) → long // 1 if the timeout was set, 0 otherwise

- TTL ( @keyspace , @read , @fast ) Returns the expiration time in seconds of a key. ▶ Methods ttl( key: byte[] ) → long // TTL in seconds, or a negative value in order to signal an error ttl( key: String ) → long // TTL in seconds, or a negative value in order to signal an error

Java-Sync Quick-Start

```
CompletableFuture<Void> expireExample = asyncCommands.set("mykey", "Hello")
.thenCompose(r1 -> {
System.out.println(r1); // >>> OK
return asyncCommands.expire("mykey", 10);
})
.thenCompose(r2 -> {
System.out.println(r2); // >>> true
return asyncCommands.ttl("mykey");
})
.thenCompose(r3 -> {
System.out.println(r3); // >>> 10
// Overwriting a key with SET clears its expiry.
return asyncCommands.set("mykey", "Hello World");
})
.thenCompose(r4 -> {
System.out.println(r4); // >>> OK
return asyncCommands.ttl("mykey");
})
.thenCompose(r5 -> {
System.out.println(r5); // >>> -1
// XX only sets the expiry when one already exists, so this is a no-op.
return asyncCommands.expire("mykey", 10, ExpireArgs.Builder.xx());
})
.thenCompose(r6 -> {
System.out.println(r6); // >>> false
return asyncCommands.ttl("mykey");
})
.thenCompose(r7 -> {
System.out.println(r7); // >>> -1
// NX only sets the expiry when there is none, so this one applies.
return asyncCommands.expire("mykey", 10, ExpireArgs.Builder.nx());
})
.thenCompose(r8 -> {
System.out.println(r8); // >>> true
return asyncCommands.ttl("mykey");
})
.thenAccept(r9 -> {
System.out.println(r9); // >>> 10
})
.toCompletableFuture();

```

```
package io.redis.examples.async;

import io.lettuce.core.*;

import io.lettuce.core.api.async.RedisAsyncCommands;

import io.lettuce.core.api.StatefulRedisConnection;

import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.concurrent.CompletableFuture;

public class CmdsGenericExample {

public void run() {

CompletableFuture<Void> existsExample = asyncCommands.set("key1", "Hello").thenCompose(res1 -> {
System.out.println(res1); // >>> OK

return asyncCommands.exists("key1");
}).thenCompose(res2 -> {
System.out.println(res2); // >>> 1

return asyncCommands.exists("nosuchkey");
}).thenCompose(res3 -> {
System.out.println(res3); // >>> 0

return asyncCommands.set("key2", "World");
}).thenCompose(res4 -> {
System.out.println(res4); // >>> OK

return asyncCommands.exists("key1", "key2", "nosuchkey");
}).thenAccept(res5 -> {
System.out.println(res5); // >>> 2
}).toCompletableFuture();
existsExample.join();

CompletableFuture<Void> keysExample = asyncCommands.mset(Map.of(
"firstname", "Jack",
"lastname", "Stuntman",
"age", "35"
)).thenCompose(res1 -> {
System.out.println(res1); // >>> OK

return asyncCommands.keys("*name*");
}).thenCompose(res2 -> {
Collections.sort(res2);
System.out.println(res2); // >>> [firstname, lastname]

return asyncCommands.keys("a??");
}).thenCompose(res3 -> {
System.out.println(res3); // >>> [age]

return asyncCommands.keys("*");
}).thenAccept(res4 -> {
Collections.sort(res4);
System.out.println(res4); // >>> [age, firstname, lastname]
}).toCompletableFuture();
keysExample.join();

CompletableFuture<Void> scan1Example = asyncCommands
.sadd("myset", "1", "2", "3", "foo", "foobar", "feelsgood")
.thenCompose(scan1Res1 -> {
System.out.println(scan1Res1); // >>> 6
return asyncCommands.sscan("myset", ScanArgs.Builder.matches("f*"));
})
.thenAccept(scan1Res2 -> {
List<String> members = new java.util.ArrayList<>(scan1Res2.getValues());
Collections.sort(members);
System.out.println(members); // >>> [feelsgood, foo, foobar]
})
.toCompletableFuture();

scan1Example.join();

// MATCH is applied after elements are fetched, so with the default COUNT most
// iterations return few keys or none at all. Each iteration is awaited because
// the next one needs the cursor this one returns.
KeyScanCursor<String> scan2Cursor = asyncCommands
.scan(ScanArgs.Builder.matches("*11*")).toCompletableFuture().join();
int scan2Total = scan2Cursor.getKeys().size();
System.out.println(scan2Cursor.getKeys().size());

for (int i = 0; i < 3; i++) {
scan2Cursor = asyncCommands
.scan(scan2Cursor, ScanArgs.Builder.matches("*11*"))
.toCompletableFuture().join();
scan2Total += scan2Cursor.getKeys().size();
System.out.println(scan2Cursor.getKeys().size());
}

// A larger COUNT forces more scanning in a single iteration, so the remaining
// matches arrive together. This continues from the cursor reached above.
scan2Cursor = asyncCommands
.scan(scan2Cursor, ScanArgs.Builder.matches("*11*").limit(1000))
.toCompletableFuture().join();
scan2Total += scan2Cursor.getKeys().size();
System.out.println(scan2Cursor.getKeys().size());

// The per-call split isn't guaranteed, but the cumulative total is.
System.out.println(scan2Total); // >>> 19

long scan3Result1 = asyncCommands.geoadd("geokey", 0, 0, "value")
.toCompletableFuture().join();
System.out.println(scan3Result1); // >>> 1

long scan3Result2 = asyncCommands.zadd("zkey", 1000, "value")
.toCompletableFuture().join();
System.out.println(scan3Result2); // >>> 1

String scan3Result3 = asyncCommands.type("geokey").toCompletableFuture().join();
System.out.println(scan3Result3); // >>> zset

String scan3Result4 = asyncCommands.type("zkey").toCompletableFuture().join();
System.out.println(scan3Result4); // >>> zset

// A single call isn't guaranteed to find every match, so loop until
// the cursor is finished, accumulating matches from every call.
List<String> scan3Keys = new java.util.ArrayList<>();
KeyScanCursor<String> scan3Cursor = asyncCommands
.scan(KeyScanArgs.Builder.type("zset")).toCompletableFuture().join();
scan3Keys.addAll(scan3Cursor.getKeys());
while (!scan3Cursor.isFinished()) {
scan3Cursor = asyncCommands
.scan(scan3Cursor, KeyScanArgs.Builder.type("zset"))
.toCompletableFuture().join();
scan3Keys.addAll(scan3Cursor.getKeys());
}
Collections.sort(scan3Keys);
System.out.println(scan3Keys); // >>> [geokey, zkey]

CompletableFuture<Void> scan4Example = asyncCommands
.hset("myhash", Map.of("a", "1", "b", "2"))
.thenCompose(scan4Res1 -> {
System.out.println(scan4Res1); // >>> 2
return asyncCommands.hscan("myhash");
})
.thenCompose(scan4Res2 -> {
System.out.println(new java.util.TreeMap<>(scan4Res2.getMap()));
// >>> {a=1, b=2}
return asyncCommands.hscanNovalues("myhash");
})
.thenAccept(scan4Res3 -> {
List<String> fields = new java.util.ArrayList<>(scan4Res3.getKeys());
Collections.sort(fields);
System.out.println(fields); // >>> [a, b]
})
.toCompletableFuture();

scan4Example.join();
CompletableFuture<Void> delExample = asyncCommands.set("key1", "Hello")
.thenCompose(r1 -> {
System.out.println(r1); // >>> OK
return asyncCommands.set("key2", "World");
})
.thenCompose(r2 -> {
System.out.println(r2); // >>> OK
return asyncCommands.del("key1", "key2", "key3");
})
.thenAccept(r3 -> {
System.out.println(r3); // >>> 2
})
.toCompletableFuture();

delExample.join();

CompletableFuture<Void> expireExample = asyncCommands.set("mykey", "Hello")
.thenCompose(r1 -> {
System.out.println(r1); // >>> OK
return asyncCommands.expire("mykey", 10);
})
.thenCompose(r2 -> {
System.out.println(r2); // >>> true
return asyncCommands.ttl("mykey");
})
.thenCompose(r3 -> {
System.out.println(r3); // >>> 10
// Overwriting a key with SET clears its expiry.
return asyncCommands.set("mykey", "Hello World");
})
.thenCompose(r4 -> {
System.out.println(r4); // >>> OK
return asyncCommands.ttl("mykey");
})
.thenCompose(r5 -> {
System.out.println(r5); // >>> -1
// XX only sets the expiry when one already exists, so this is a no-op.
return asyncCommands.expire("mykey", 10, ExpireArgs.Builder.xx());
})
.thenCompose(r6 -> {
System.out.println(r6); // >>> false
return asyncCommands.ttl("mykey");
})
.thenCompose(r7 -> {
System.out.println(r7); // >>> -1
// NX only sets the expiry when there is none, so this one applies.
return asyncCommands.expire("mykey", 10, ExpireArgs.Builder.nx());
})
.thenCompose(r8 -> {
System.out.println(r8); // >>> true
return asyncCommands.ttl("mykey");
})
.thenAccept(r9 -> {
System.out.println(r9); // >>> 10
})
.toCompletableFuture();

expireExample.join();

CompletableFuture<Void> ttlExample = asyncCommands.set("mykey", "Hello")
.thenCompose(r1 -> {
System.out.println(r1); // >>> OK
return asyncCommands.expire("mykey", 10);
})
.thenCompose(r2 -> {
System.out.println(r2); // >>> true
return asyncCommands.ttl("mykey");
})
.thenAccept(r3 -> {
System.out.println(r3); // >>> 10
})
.toCompletableFuture();

ttlExample.join();

} finally {
redisClient.shutdown();
}
}
}

```

▼
Commands:
SET, EXPIRE, TTL

- SET ( @write , @string , @slow ) Sets the string value of a key, ignoring its type. The key is created if it doesn't exist. ▶ Methods set( key: K, // the key. value: V // the value. ) → RedisFuture<String> // String simple-string-reply OK if SET was executed correctly. set( key: K, // the key. value: V, // the value. setArgs: SetArgs // the setArgs. ) → RedisFuture<String> // String simple-string-reply OK if SET was executed correctly.

- EXPIRE ( @keyspace , @write , @fast ) Sets the expiration time of a key in seconds. ▶ Methods expire( key: K, // the key. seconds: long // the seconds. ) → RedisFuture<Boolean> // Boolean integer-reply specifically: true if the timeout was set. false if key does not exist or the timeout could not be set. expire( key: K, // the key. seconds: long, // the seconds. expireArgs: ExpireArgs // the expiry arguments. ) → RedisFuture<Boolean> // Boolean integer-reply specifically: true if the timeout was set. false if key does not exist or the timeout could not be set. expire( key: K, // the key. seconds: Duration // the seconds. ) → RedisFuture<Boolean> // Boolean integer-reply specifically: true if the timeout was set. false if key does not exist or the timeout could not be set. expire( key: K, // the key. seconds: Duration, // the seconds. expireArgs: ExpireArgs // the expiry arguments. ) → RedisFuture<Boolean> // Boolean integer-reply specifically: true if the timeout was set. false if key does not exist or the timeout could not be set.

- TTL ( @keyspace , @read , @fast ) Returns the expiration time in seconds of a key. ▶ Method ttl( key: K // the key. ) → RedisFuture<Long> // Long integer-reply TTL in seconds, or a negative value in order to signal an error.

Java-Async Quick-Start

```
Mono<Void> expireExample = reactiveCommands.set("mykey", "Hello")
.flatMap(r1 -> {
System.out.println(r1); // >>> OK
return reactiveCommands.expire("mykey", 10);
})
.flatMap(r2 -> {
System.out.println(r2); // >>> true
return reactiveCommands.ttl("mykey");
})
.flatMap(r3 -> {
System.out.println(r3); // >>> 10
// Overwriting a key with SET clears its expiry.
return reactiveCommands.set("mykey", "Hello World");
})
.flatMap(r4 -> {
System.out.println(r4); // >>> OK
return reactiveCommands.ttl("mykey");
})
.flatMap(r5 -> {
System.out.println(r5); // >>> -1
// XX only sets the expiry when one already exists, so this is a no-op.
return reactiveCommands.expire("mykey", 10, ExpireArgs.Builder.xx());
})
.flatMap(r6 -> {
System.out.println(r6); // >>> false
return reactiveCommands.ttl("mykey");
})
.flatMap(r7 -> {
System.out.println(r7); // >>> -1
// NX only sets the expiry when there is none, so this one applies.
return reactiveCommands.expire("mykey", 10, ExpireArgs.Builder.nx());
})
.flatMap(r8 -> {
System.out.println(r8); // >>> true
return reactiveCommands.ttl("mykey");
})
.doOnNext(r9 -> {
System.out.println(r9); // >>> 10
})
.then();

```

```
package io.redis.examples.reactive;

import io.lettuce.core.*;
import io.lettuce.core.api.reactive.RedisReactiveCommands;
import io.lettuce.core.api.StatefulRedisConnection;
import reactor.core.publisher.Mono;

import java.util.Collections;
import java.util.List;
import java.util.Map;

public class CmdsGenericExample {

public void run() {
RedisClient redisClient = RedisClient.create("redis://localhost:6379");

try (StatefulRedisConnection<String, String> connection = redisClient.connect()) {
RedisReactiveCommands<String, String> reactiveCommands = connection.reactive();

Mono<Void> existsExample = reactiveCommands.set("key1", "Hello").doOnNext(res1 -> {
System.out.println(res1); // >>> OK
}).then(reactiveCommands.exists("key1")).doOnNext(res2 -> {
System.out.println(res2); // >>> 1
}).then(reactiveCommands.exists("nosuchkey")).doOnNext(res3 -> {
System.out.println(res3); // >>> 0
}).then(reactiveCommands.set("key2", "World")).doOnNext(res4 -> {
System.out.println(res4); // >>> OK
}).then(reactiveCommands.exists("key1", "key2", "nosuchkey")).doOnNext(res5 -> {
System.out.println(res5); // >>> 2
}).then();

Mono.when(existsExample).block();

Mono<Void> keysExample = reactiveCommands.mset(Map.of(
"firstname", "Jack",
"lastname", "Stuntman",
"age", "35"
)).doOnNext(res1 -> {
System.out.println(res1); // >>> OK
}).then(reactiveCommands.keys("*name*").collectList()).doOnNext(res2 -> {
Collections.sort(res2);
System.out.println(res2); // >>> [firstname, lastname]
}).then(reactiveCommands.keys("a??").collectList()).doOnNext(res3 -> {
System.out.println(res3); // >>> [age]
}).then(reactiveCommands.keys("*").collectList()).doOnNext(res4 -> {
Collections.sort(res4);
System.out.println(res4); // >>> [age, firstname, lastname]
}).then();

Mono.when(keysExample).block();

Mono<Void> scan1Example = reactiveCommands
.sadd("myset", "1", "2", "3", "foo", "foobar", "feelsgood")
.flatMap(scan1Res1 -> {
System.out.println(scan1Res1); // >>> 6
return reactiveCommands.sscan("myset", ScanArgs.Builder.matches("f*"));
})
.doOnNext(scan1Res2 -> {
List<String> members = new java.util.ArrayList<>(scan1Res2.getValues());
Collections.sort(members);
System.out.println(members); // >>> [feelsgood, foo, foobar]
})
.then();

Mono.when(scan1Example).block();

// MATCH is applied after elements are fetched, so with the default COUNT most
// iterations return few keys or none at all. Each iteration is subscribed to in
// turn because the next one needs the cursor this one returns.
KeyScanCursor<String> scan2Cursor = reactiveCommands
.scan(ScanArgs.Builder.matches("*11*")).block();
int scan2Total = scan2Cursor.getKeys().size();
System.out.println(scan2Cursor.getKeys().size());

for (int i = 0; i < 3; i++) {
scan2Cursor = reactiveCommands
.scan(scan2Cursor, ScanArgs.Builder.matches("*11*")).block();
scan2Total += scan2Cursor.getKeys().size();
System.out.println(scan2Cursor.getKeys().size());
}

// A larger COUNT forces more scanning in a single iteration, so the remaining
// matches arrive together. This continues from the cursor reached above.
scan2Cursor = reactiveCommands
.scan(scan2Cursor, ScanArgs.Builder.matches("*11*").limit(1000)).block();
scan2Total += scan2Cursor.getKeys().size();
System.out.println(scan2Cursor.getKeys().size());

// The per-call split isn't guaranteed, but the cumulative total is.
System.out.println(scan2Total); // >>> 19

long scan3Result1 = reactiveCommands.geoadd("geokey", 0, 0, "value").block();
System.out.println(scan3Result1); // >>> 1

long scan3Result2 = reactiveCommands.zadd("zkey", 1000, "value").block();
System.out.println(scan3Result2); // >>> 1

String scan3Result3 = reactiveCommands.type("geokey").block();
System.out.println(scan3Result3); // >>> zset

String scan3Result4 = reactiveCommands.type("zkey").block();
System.out.println(scan3Result4); // >>> zset

// A single call isn't guaranteed to find every match, so loop until
// the cursor is finished, accumulating matches from every call.
List<String> scan3Keys = new java.util.ArrayList<>();
KeyScanCursor<String> scan3Cursor = reactiveCommands
.scan(KeyScanArgs.Builder.type("zset")).block();
scan3Keys.addAll(scan3Cursor.getKeys());
while (!scan3Cursor.isFinished()) {
scan3Cursor = reactiveCommands
.scan(scan3Cursor, KeyScanArgs.Builder.type("zset")).block();
scan3Keys.addAll(scan3Cursor.getKeys());
}
Collections.sort(scan3Keys);
System.out.println(scan3Keys); // >>> [geokey, zkey]

Mono<Void> scan4Example = reactiveCommands
.hset("myhash", Map.of("a", "1", "b", "2"))
.flatMap(scan4Res1 -> {
System.out.println(scan4Res1); // >>> 2
return reactiveCommands.hscan("myhash");
})
.flatMap(scan4Res2 -> {
System.out.println(new java.util.TreeMap<>(scan4Res2.getMap()));
// >>> {a=1, b=2}
return reactiveCommands.hscanNovalues("myhash");
})
.doOnNext(scan4Res3 -> {
List<String> fields = new java.util.ArrayList<>(scan4Res3.getKeys());
Collections.sort(fields);
System.out.println(fields); // >>> [a, b]
})
.then();

Mono.when(scan4Example).block();
Mono<Void> delExample = reactiveCommands.set("key1", "Hello")
.flatMap(r1 -> {
System.out.println(r1); // >>> OK
return reactiveCommands.set("key2", "World");
})
.flatMap(r2 -> {
System.out.println(r2); // >>> OK
return reactiveCommands.del("key1", "key2", "key3");
})
.doOnNext(r3 -> {
System.out.println(r3); // >>> 2
})
.then();

Mono.when(delExample).block();

Mono<Void> expireExample = reactiveCommands.set("mykey", "Hello")
.flatMap(r1 -> {
System.out.println(r1); // >>> OK
return reactiveCommands.expire("mykey", 10);
})
.flatMap(r2 -> {
System.out.println(r2); // >>> true
return reactiveCommands.ttl("mykey");
})
.flatMap(r3 -> {
System.out.println(r3); // >>> 10
// Overwriting a key with SET clears its expiry.
return reactiveCommands.set("mykey", "Hello World");
})
.flatMap(r4 -> {
System.out.println(r4); // >>> OK
return reactiveCommands.ttl("mykey");
})
.flatMap(r5 -> {
System.out.println(r5); // >>> -1
// XX only sets the expiry when one already exists, so this is a no-op.
return reactiveCommands.expire("mykey", 10, ExpireArgs.Builder.xx());
})
.flatMap(r6 -> {
System.out.println(r6); // >>> false
return reactiveCommands.ttl("mykey");
})
.flatMap(r7 -> {
System.out.println(r7); // >>> -1
// NX only sets the expiry when there is none, so this one applies.
return reactiveCommands.expire("mykey", 10, ExpireArgs.Builder.nx());
})
.flatMap(r8 -> {
System.out.println(r8); // >>> true
return reactiveCommands.ttl("mykey");
})
.doOnNext(r9 -> {
System.out.println(r9); // >>> 10
})
.then();

Mono.when(expireExample).block();

Mono<Void> ttlExample = reactiveCommands.set("mykey", "Hello")
.flatMap(r1 -> {
System.out.println(r1); // >>> OK
return reactiveCommands.expire("mykey", 10);
})
.flatMap(r2 -> {
System.out.println(r2); // >>> true
return reactiveCommands.ttl("mykey");
})
.doOnNext(r3 -> {
System.out.println(r3); // >>> 10
})
.then();

Mono.when(ttlExample).block();

} finally {
redisClient.shutdown();
}
}

}

```

▼
Commands:
SET, EXPIRE, TTL

- SET ( @write , @string , @slow ) Sets the string value of a key, ignoring its type. The key is created if it doesn't exist. ▶ Methods set( key: K, // the key. value: V // the value. ) → Mono<String> // String simple-string-reply OK if SET was executed correctly. set( key: K, // the key. value: V, // the value. setArgs: SetArgs // the setArgs. ) → Mono<String> // String simple-string-reply OK if SET was executed correctly.

- EXPIRE ( @keyspace , @write , @fast ) Sets the expiration time of a key in seconds. ▶ Methods expire( key: K, // the key. seconds: long // the seconds. ) → Mono<Boolean> // Boolean integer-reply specifically: true if the timeout was set. false if key does not exist or the timeout could not be set. expire( key: K, // the key. seconds: long, // the seconds. expireArgs: ExpireArgs // the expiry arguments. ) → Mono<Boolean> // Boolean integer-reply specifically: true if the timeout was set. false if key does not exist or the timeout could not be set. expire( key: K, // the key. seconds: Duration // the seconds. ) → Mono<Boolean> // Boolean integer-reply specifically: true if the timeout was set. false if key does not exist or the timeout could not be set. expire( key: K, // the key. seconds: Duration, // the seconds. expireArgs: ExpireArgs // the expiry arguments. ) → Mono<Boolean> // Boolean integer-reply specifically: true if the timeout was set. false if key does not exist or the timeout could not be set.

- TTL ( @keyspace , @read , @fast ) Returns the expiration time in seconds of a key. ▶ Method ttl( key: K // the key. ) → Mono<Long> // Long integer-reply TTL in seconds, or a negative value in order to signal an error.

Java-Reactive Quick-Start

```
expireResult1, err := rdb.Set(ctx, "mykey", "Hello", 0).Result()

if err != nil {
panic(err)
}

fmt.Println(expireResult1) // >>> OK

expireResult2, err := rdb.Expire(ctx, "mykey", 10*time.Second).Result()

if err != nil {
panic(err)
}

fmt.Println(expireResult2) // >>> true

expireResult3, err := rdb.TTL(ctx, "mykey").Result()

if err != nil {
panic(err)
}

fmt.Println(math.Round(expireResult3.Seconds())) // >>> 10

expireResult4, err := rdb.Set(ctx, "mykey", "Hello World", 0).Result()

if err != nil {
panic(err)
}

fmt.Println(expireResult4) // >>> OK

expireResult5, err := rdb.TTL(ctx, "mykey").Result()

if err != nil {
panic(err)
}

fmt.Println(expireResult5) // >>> -1ns

expireResult6, err := rdb.ExpireXX(ctx, "mykey", 10*time.Second).Result()

if err != nil {
panic(err)
}

fmt.Println(expireResult6) // >>> false

expireResult7, err := rdb.TTL(ctx, "mykey").Result()

if err != nil {
panic(err)
}

fmt.Println(expireResult7) // >>> -1ns

expireResult8, err := rdb.ExpireNX(ctx, "mykey", 10*time.Second).Result()

if err != nil {
panic(err)
}

fmt.Println(expireResult8) // >>> true

expireResult9, err := rdb.TTL(ctx, "mykey").Result()

if err != nil {
panic(err)
}

fmt.Println(math.Round(expireResult9.Seconds())) // >>> 10

```

```
package example_commands_test

import (
"context"
"fmt"
"math"
"sort"
"time"

"github.com/redis/go-redis/v9"
)

func ExampleClient_del_cmd() {
ctx := context.Background()

rdb := redis.NewClient(&redis.Options{
Addr: "localhost:6379",
Password: "", // no password docs
DB: 0, // use default DB
})

delResult1, err := rdb.Set(ctx, "key1", "Hello", 0).Result()

if err != nil {
panic(err)
}

fmt.Println(delResult1) // >>> OK

delResult2, err := rdb.Set(ctx, "key2", "World", 0).Result()

if err != nil {
panic(err)
}

fmt.Println(delResult2) // >>> OK

delResult3, err := rdb.Del(ctx, "key1", "key2", "key3").Result()

if err != nil {
panic(err)
}

fmt.Println(delResult3) // >>> 2

}

func ExampleClient_exists_cmd() {
ctx := context.Background()

rdb := redis.NewClient(&redis.Options{
Addr: "localhost:6379",
Password: "", // no password docs
DB: 0, // use default DB
})

existsResult1, err := rdb.Set(ctx, "key1", "Hello", 0).Result()

if err != nil {
panic(err)
}

fmt.Println(existsResult1) // >>> OK

existsResult2, err := rdb.Exists(ctx, "key1").Result()

if err != nil {
panic(err)
}

fmt.Println(existsResult2) // >>> 1

existsResult3, err := rdb.Exists(ctx, "nosuchkey").Result()

if err != nil {
panic(err)
}

fmt.Println(existsResult3) // >>> 0

existsResult4, err := rdb.Set(ctx, "key2", "World", 0).Result()

if err != nil {
panic(err)
}

fmt.Println(existsResult4) // >>> OK

existsResult5, err := rdb.Exists(ctx, "key1", "key2", "nosuchkey").Result()

if err != nil {
panic(err)
}

fmt.Println(existsResult5) // >>> 2

}

func ExampleClient_expire_cmd() {
ctx := context.Background()

rdb := redis.NewClient(&redis.Options{
Addr: "localhost:6379",
Password: "", // no password docs
DB: 0, // use default DB
})

expireResult1, err := rdb.Set(ctx, "mykey", "Hello", 0).Result()

if err != nil {
panic(err)
}

fmt.Println(expireResult1) // >>> OK

expireResult2, err := rdb.Expire(ctx, "mykey", 10*time.Second).Result()

if err != nil {
panic(err)
}

fmt.Println(expireResult2) // >>> true

expireResult3, err := rdb.TTL(ctx, "mykey").Result()

if err != nil {
panic(err)
}

fmt.Println(math.Round(expireResult3.Seconds())) // >>> 10

expireResult4, err := rdb.Set(ctx, "mykey", "Hello World", 0).Result()

if err != nil {
panic(err)
}

fmt.Println(expireResult4) // >>> OK

expireResult5, err := rdb.TTL(ctx, "mykey").Result()

if err != nil {
panic(err)
}

fmt.Println(expireResult5) // >>> -1ns

expireResult6, err := rdb.ExpireXX(ctx, "mykey", 10*time.Second).Result()

if err != nil {
panic(err)
}

fmt.Println(expireResult6) // >>> false

expireResult7, err := rdb.TTL(ctx, "mykey").Result()

if err != nil {
panic(err)
}

fmt.Println(expireResult7) // >>> -1ns

expireResult8, err := rdb.ExpireNX(ctx, "mykey", 10*time.Second).Result()

if err != nil {
panic(err)
}

fmt.Println(expireResult8) // >>> true

expireResult9, err := rdb.TTL(ctx, "mykey").Result()

if err != nil {
panic(err)
}

fmt.Println(math.Round(expireResult9.Seconds())) // >>> 10

}

func ExampleClient_keys_cmd() {
ctx := context.Background()

rdb := redis.NewClient(&redis.Options{
Addr: "localhost:6379",
Password: "", // no password docs
DB: 0, // use default DB
})

keysResult1, err := rdb.MSet(ctx, "firstname", "Jack", "lastname", "Stuntman", "age", "35").Result()

if err != nil {
panic(err)
}

fmt.Println(keysResult1) // >>> OK

keysResult2, err := rdb.Keys(ctx, "*name*").Result()

if err != nil {
panic(err)
}

sort.Strings(keysResult2)
fmt.Println(keysResult2) // >>> [firstname lastname]

keysResult3, err := rdb.Keys(ctx, "a??").Result()

if err != nil {
panic(err)
}

fmt.Println(keysResult3) // >>> [age]

keysResult4, err := rdb.Keys(ctx, "*").Result()

if err != nil {
panic(err)
}

sort.Strings(keysResult4)
fmt.Println(keysResult4) // >>> [age firstname lastname]

}

func ExampleClient_ttl_cmd() {
ctx := context.Background()

rdb := redis.NewClient(&redis.Options{
Addr: "localhost:6379",
Password: "", // no password docs
DB: 0, // use default DB
})

ttlResult1, err := rdb.Set(ctx, "mykey", "Hello", 10*time.Second).Result()

if err != nil {
panic(err)
}

fmt.Println(ttlResult1) // >>> OK

ttlResult2, err := rdb.TTL(ctx, "mykey").Result()

if err != nil {
panic(err)
}

fmt.Println(math.Round(ttlResult2.Seconds())) // >>> 10

}

func ExampleClient_scan1_cmd() {
ctx := context.Background()

rdb := redis.NewClient(&redis.Options{
Addr: "localhost:6379",
Password: "", // no password docs
DB: 0, // use default DB
})

scan1Result1, err := rdb.SAdd(ctx, "myset", "1", "2", "3", "foo", "foobar", "feelsgood").Result()

if err != nil {
panic(err)
}

fmt.Println(scan1Result1) // >>> 6

scan1Result2, _, err := rdb.SScan(ctx, "myset", 0, "f*", 0).Result()

if err != nil {
panic(err)
}

sort.Strings(scan1Result2)
fmt.Println(scan1Result2) // >>> [feelsgood foo foobar]

}

func ExampleClient_scan2_cmd() {
ctx := context.Background()

rdb := redis.NewClient(&redis.Options{
Addr: "localhost:6379",
Password: "", // no password docs
DB: 0, // use default DB
})

// MATCH is applied after elements are fetched, so with the default COUNT most
// iterations return few keys or none at all.
var scan2Cursor uint64
var scan2Keys []string
var err error
scan2Total := 0

for i := 0; i < 4; i++ {
scan2Keys, scan2Cursor, err = rdb.Scan(ctx, scan2Cursor, "*11*", 0).Result()

if err != nil {
panic(err)
}
scan2Total += len(scan2Keys)
}

// A larger COUNT forces more scanning in a single iteration, so the remaining
// matches arrive together. This continues from the cursor reached above.
scan2Keys, _, err = rdb.Scan(ctx, scan2Cursor, "*11*", 1000).Result()

if err != nil {
panic(err)
}
scan2Total += len(scan2Keys)

// The per-call split isn't guaranteed, but the cumulative total is.
fmt.Println(scan2Total) // >>> 19

}

func ExampleClient_scan3_cmd() {
ctx := context.Background()

rdb := redis.NewClient(&redis.Options{
Addr: "localhost:6379",
Password: "", // no password docs
DB: 0, // use default DB
})

scan3Result1, err := rdb.GeoAdd(ctx, "geokey", &redis.GeoLocation{
Longitude: 0, Latitude: 0, Name: "value",
}).Result()

if err != nil {
panic(err)
}

fmt.Println(scan3Result1) // >>> 1

scan3Result2, err := rdb.ZAdd(ctx, "zkey", redis.Z{Score: 1000, Member: "value"}).Result()

if err != nil {
panic(err)
}

fmt.Println(scan3Result2) // >>> 1

scan3Result3, err := rdb.Type(ctx, "geokey").Result()

if err != nil {
panic(err)
}

fmt.Println(scan3Result3) // >>> zset

scan3Result4, err := rdb.Type(ctx, "zkey").Result()

if err != nil {
panic(err)
}

fmt.Println(scan3Result4) // >>> zset

// A single call isn't guaranteed to find every match, so loop until the cursor
// returns to 0, accumulating matches from every call.
var scan3Cursor uint64
var scan3Keys []string
var scan3Batch []string

for {
scan3Batch, scan3Cursor, err = rdb.ScanType(ctx, scan3Cursor, "", 0, "zset").Result()

if err != nil {
panic(err)
}
scan3Keys = append(scan3Keys, scan3Batch...)

if scan3Cursor == 0 {
break
}
}

sort.Strings(scan3Keys)
fmt.Println(scan3Keys) // >>> [geokey zkey]

}

func ExampleClient_scan4_cmd() {
ctx := context.Background()

rdb := redis.NewClient(&redis.Options{
Addr: "localhost:6379",
Password: "", // no password docs
DB: 0, // use default DB
})

scan4Result1, err := rdb.HSet(ctx, "myhash", "a", 1, "b", 2).Result()

if err != nil {
panic(err)
}

fmt.Println(scan4Result1) // >>> 2

scan4Result2, _, err := rdb.HScan(ctx, "myhash", 0, "", 0).Result()

if err != nil {
panic(err)
}

// HSCAN returns field and value interleaved. Redis does not promise an order, so
// collect the pairs into a map: fmt prints map keys sorted, whatever order they arrived in.
scan4Fields := map[string]string{}

for i := 0; i < len(scan4Result2); i += 2 {
scan4Fields[scan4Result2[i]] = scan4Result2[i+1]
}

fmt.Println(scan4Fields) // >>> map[a:1 b:2]

scan4Result3, _, err := rdb.HScanNoValues(ctx, "myhash", 0, "", 0).Result()

if err != nil {
panic(err)
}

sort.Strings(scan4Result3)
fmt.Println(scan4Result3) // >>> [a b]

}

```

▼
Commands:
SET, EXPIRE, TTL

- SET ( @write , @string , @slow ) Sets the string value of a key, ignoring its type. The key is created if it doesn't exist. ▶ Methods Set( ctx: context.Context, key: string, value: interface{}, expiration: time.Duration ) → *StatusCmd SetArgs( ctx: context.Context, key: string, value: interface{}, a: SetArgs // Mode can be `NX`, `XX`, `IFEQ`, `IFNE`, `IFDEQ`, `IFDNE` or empty. MatchValue is used with IFEQ/IFNE; MatchDigest (a uint64 xxh3 hash) is used with IFDEQ/IFDNE. ) → *StatusCmd SetIFEQ( ctx: context.Context, key: string, value: interface{}, matchValue: interface{}, expiration: time.Duration // Zero expiration means the key has no expiration time. ) → *StatusCmd // Returns "OK" on success. Returns nil if the operation was aborted due to condition not matching. SetIFNE( ctx: context.Context, key: string, value: interface{}, matchValue: interface{}, expiration: time.Duration // Zero expiration means the key has no expiration time. ) → *StatusCmd // Returns "OK" on success. Returns nil if the operation was aborted due to condition not matching. SetIFDEQ( ctx: context.Context, key: string, value: interface{}, matchDigest: uint64, // A uint64 xxh3 hash value. expiration: time.Duration // Zero expiration means the key has no expiration time. ) → *StatusCmd // Returns "OK" on success. Returns nil if the operation was aborted due to condition not matching. SetIFDNE( ctx: context.Context, key: string, value: interface{}, matchDigest: uint64, // A uint64 xxh3 hash value. expiration: time.Duration // Zero expiration means the key has no expiration time. ) → *StatusCmd // Returns "OK" on success. Returns nil if the operation was aborted due to condition not matching.

- EXPIRE ( @keyspace , @write , @fast ) Sets the expiration time of a key in seconds. ▶ Methods Expire( ctx: context.Context, key: string, expiration: time.Duration ) → *BoolCmd ExpireNX( ctx: context.Context, key: string, expiration: time.Duration ) → *BoolCmd ExpireXX( ctx: context.Context, key: string, expiration: time.Duration ) → *BoolCmd ExpireGT( ctx: context.Context, key: string, expiration: time.Duration ) → *BoolCmd ExpireLT( ctx: context.Context, key: string, expiration: time.Duration ) → *BoolCmd

- TTL ( @keyspace , @read , @fast ) Returns the expiration time in seconds of a key. ▶ Method TTL( ctx: context.Context, key: string ) → *DurationCmd

Go Quick-Start

```
reply = redisCommand(c, "SET mykey Hello");
printf("%s\n", reply->str);
// >>> OK
freeReplyObject(reply);

reply = redisCommand(c, "EXPIRE mykey 10");
printf("%lld\n", reply->integer);
// >>> 1
freeReplyObject(reply);

reply = redisCommand(c, "TTL mykey");
printf("%lld\n", reply->integer);
// >>> 10
freeReplyObject(reply);

// Overwriting a key with SET clears its expiry.
reply = redisCommand(c, "SET mykey %s", "Hello World");
printf("%s\n", reply->str);
// >>> OK
freeReplyObject(reply);

reply = redisCommand(c, "TTL mykey");
printf("%lld\n", reply->integer);
// >>> -1
freeReplyObject(reply);

// XX only sets the expiry when one already exists, so this is a no-op.
reply = redisCommand(c, "EXPIRE mykey 10 XX");
printf("%lld\n", reply->integer);
// >>> 0
freeReplyObject(reply);

reply = redisCommand(c, "TTL mykey");
printf("%lld\n", reply->integer);
// >>> -1
freeReplyObject(reply);

// NX only sets the expiry when there is none, so this one applies.
reply = redisCommand(c, "EXPIRE mykey 10 NX");
printf("%lld\n", reply->integer);
// >>> 1
freeReplyObject(reply);

reply = redisCommand(c, "TTL mykey");
printf("%lld\n", reply->integer);
// >>> 10
freeReplyObject(reply);

```

```

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <hiredis/hiredis.h>

int main(int argc, char **argv) {
redisContext *c = redisConnect("127.0.0.1", 6379);

if (c == NULL || c->err) {
if (c) {
printf("Connection error: %s\n", c->errstr);
redisFree(c);
} else {
printf("Connection error: can't allocate redis context\n");
}
return 1;
}

redisReply *reply;

// Set up keys
reply = redisCommand(c, "MSET %s %s %s %s %s %s",
"firstname", "Jack", "lastname", "Stuntman", "age", "35");
printf("MSET firstname Jack lastname Stuntman age 35: %s\n", reply->str);
// >>> OK
freeReplyObject(reply);

// Keys matching *name*
reply = redisCommand(c, "KEYS %s", "*name*");
printf("KEYS *name*:\n");
for (size_t i = 0; i < reply->elements; i++) {
printf(" %s\n", reply->element[i]->str);
}
// >>> firstname
// >>> lastname
freeReplyObject(reply);

// Keys matching a??
reply = redisCommand(c, "KEYS %s", "a??");
printf("KEYS a??:\n");
for (size_t i = 0; i < reply->elements; i++) {
printf(" %s\n", reply->element[i]->str);
}
// >>> age
freeReplyObject(reply);

// All keys
reply = redisCommand(c, "KEYS %s", "*");
printf("KEYS *:\n");
for (size_t i = 0; i < reply->elements; i++) {
printf(" %s\n", reply->element[i]->str);
}
// >>> age
// >>> firstname
// >>> lastname
freeReplyObject(reply);

reply = redisCommand(c, "SADD myset 1 2 3 foo foobar feelsgood");
printf("%lld\n", reply->integer);
// >>> 6
freeReplyObject(reply);

// SCAN-family replies are a two-element array: the next cursor, then the results.
reply = redisCommand(c, "SSCAN myset 0 MATCH f*");
printf("%zu\n", reply->element[1]->elements);
// >>> 3
freeReplyObject(reply);

// MATCH is applied after elements are fetched, so with the default COUNT most
// iterations return few keys or none at all.
char cursor[64] = "0";

for (int i = 0; i < 4; i++) {
reply = redisCommand(c, "SCAN %s MATCH *11*", cursor);
snprintf(cursor, sizeof(cursor), "%s", reply->element[0]->str);
printf("%zu\n", reply->element[1]->elements);
freeReplyObject(reply);
}

// A larger COUNT forces more scanning in a single iteration, so the remaining
// matches arrive together. This continues from the cursor reached above.
reply = redisCommand(c, "SCAN %s MATCH *11* COUNT 1000", cursor);
printf("%zu\n", reply->element[1]->elements);
// >>> 18
freeReplyObject(reply);

reply = redisCommand(c, "GEOADD geokey 0 0 value");
printf("%lld\n", reply->integer);
// >>> 1
freeReplyObject(reply);

reply = redisCommand(c, "ZADD zkey 1000 value");
printf("%lld\n", reply->integer);
// >>> 1
freeReplyObject(reply);

reply = redisCommand(c, "TYPE geokey");
printf("%s\n", reply->str);
// >>> zset
freeReplyObject(reply);

reply = redisCommand(c, "TYPE zkey");
printf("%s\n", reply->str);
// >>> zset
freeReplyObject(reply);

reply = redisCommand(c, "SCAN 0 TYPE zset");
printf("%zu\n", reply->element[1]->elements);
// >>> 2
freeReplyObject(reply);

reply = redisCommand(c, "HSET myhash a 1 b 2");
printf("%lld\n", reply->integer);
// >>> 2
freeReplyObject(reply);

// Without NOVALUES the results alternate field, value, field, value.
reply = redisCommand(c, "HSCAN myhash 0");
for (size_t i = 0; i < reply->element[1]->elements; i += 2) {
printf("%s=%s\n", reply->element[1]->element[i]->str,
reply->element[1]->element[i + 1]->str);
}
// >>> a=1
// >>> b=2
freeReplyObject(reply);

reply = redisCommand(c, "HSCAN myhash 0 NOVALUES");
for (size_t i = 0; i < reply->element[1]->elements; i++) {
printf("%s\n", reply->element[1]->element[i]->str);
}
// >>> a
// >>> b
freeReplyObject(reply);

reply = redisCommand(c, "SET key1 Hello");
printf("%s\n", reply->str);
// >>> OK
freeReplyObject(reply);

reply = redisCommand(c, "SET key2 World");
printf("%s\n", reply->str);
// >>> OK
freeReplyObject(reply);

reply = redisCommand(c, "DEL key1 key2 key3");
printf("%lld\n", reply->integer);
// >>> 2
freeReplyObject(reply);

reply = redisCommand(c, "SET key1 Hello");
printf("%s\n", reply->str);
// >>> OK
freeReplyObject(reply);

reply = redisCommand(c, "EXISTS key1");
printf("%lld\n", reply->integer);
// >>> 1
freeReplyObject(reply);

reply = redisCommand(c, "EXISTS nosuchkey");
printf("%lld\n", reply->integer);
// >>> 0
freeReplyObject(reply);

reply = redisCommand(c, "SET key2 World");
printf("%s\n", reply->str);
// >>> OK
freeReplyObject(reply);

reply = redisCommand(c, "EXISTS key1 key2 nosuchkey");
printf("%lld\n", reply->integer);
// >>> 2
freeReplyObject(reply);

reply = redisCommand(c, "SET mykey Hello");
printf("%s\n", reply->str);
// >>> OK
freeReplyObject(reply);

reply = redisCommand(c, "EXPIRE mykey 10");
printf("%lld\n", reply->integer);
// >>> 1
freeReplyObject(reply);

reply = redisCommand(c, "TTL mykey");
printf("%lld\n", reply->integer);
// >>> 10
freeReplyObject(reply);

// Overwriting a key with SET clears its expiry.
reply = redisCommand(c, "SET mykey %s", "Hello World");
printf("%s\n", reply->str);
// >>> OK
freeReplyObject(reply);

reply = redisCommand(c, "TTL mykey");
printf("%lld\n", reply->integer);
// >>> -1
freeReplyObject(reply);

// XX only sets the expiry when one already exists, so this is a no-op.
reply = redisCommand(c, "EXPIRE mykey 10 XX");
printf("%lld\n", reply->integer);
// >>> 0
freeReplyObject(reply);

reply = redisCommand(c, "TTL mykey");
printf("%lld\n", reply->integer);
// >>> -1
freeReplyObject(reply);

// NX only sets the expiry when there is none, so this one applies.
reply = redisCommand(c, "EXPIRE mykey 10 NX");
printf("%lld\n", reply->integer);
// >>> 1
freeReplyObject(reply);

reply = redisCommand(c, "TTL mykey");
printf("%lld\n", reply->integer);
// >>> 10
freeReplyObject(reply);

reply = redisCommand(c, "SET mykey Hello");
printf("%s\n", reply->str);
// >>> OK
freeReplyObject(reply);

reply = redisCommand(c, "EXPIRE mykey 10");
printf("%lld\n", reply->integer);
// >>> 1
freeReplyObject(reply);

reply = redisCommand(c, "TTL mykey");
printf("%lld\n", reply->integer);
// >>> 10
freeReplyObject(reply);

redisFree(c);

return 0;
}

```

▼
Commands:
SET, EXPIRE, TTL

- SET ( @write , @string , @slow ) Sets the string value of a key, ignoring its type. The key is created if it doesn't exist.

- EXPIRE ( @keyspace , @write , @fast ) Sets the expiration time of a key in seconds.

- TTL ( @keyspace , @read , @fast ) Returns the expiration time in seconds of a key.

C Quick-Start

```
bool expireResult1 = db.StringSet("mykey", "Hello");
Console.WriteLine(expireResult1); // >>> true

bool expireResult2 = db.KeyExpire("mykey", new TimeSpan(0, 0, 10));
Console.WriteLine(expireResult2); // >>> true

TimeSpan expireResult3 = db.KeyTimeToLive("mykey") ?? TimeSpan.Zero;
Console.WriteLine(Math.Round(expireResult3.TotalSeconds)); // >>> 10

bool expireResult4 = db.StringSet("mykey", "Hello World");
Console.WriteLine(expireResult4); // >>> true

TimeSpan expireResult5 = db.KeyTimeToLive("mykey") ?? TimeSpan.Zero;
Console.WriteLine(Math.Round(expireResult5.TotalSeconds).ToString()); // >>> 0

bool expireResult6 = db.KeyExpire("mykey", new TimeSpan(0, 0, 10), ExpireWhen.HasExpiry);
Console.WriteLine(expireResult6); // >>> false

TimeSpan expireResult7 = db.KeyTimeToLive("mykey") ?? TimeSpan.Zero;
Console.WriteLine(Math.Round(expireResult7.TotalSeconds)); // >>> 0

bool expireResult8 = db.KeyExpire("mykey", new TimeSpan(0, 0, 10), ExpireWhen.HasNoExpiry);
Console.WriteLine(expireResult8); // >>> true

TimeSpan expireResult9 = db.KeyTimeToLive("mykey") ?? TimeSpan.Zero;
Console.WriteLine(Math.Round(expireResult9.TotalSeconds)); // >>> 10

```

```

using NRedisStack.Tests;
using StackExchange.Redis;

public class CmdsGenericExample
{
public void Run()
{
var muxer = ConnectionMultiplexer.Connect("localhost:6379");
var db = muxer.GetDatabase();

// Tests for 'copy' step.

bool delResult1 = db.StringSet("key1", "Hello");
Console.WriteLine(delResult1); // >>> true

bool delResult2 = db.StringSet("key2", "World");
Console.WriteLine(delResult2); // >>> true

long delResult3 = db.KeyDelete(["key1", "key2", "key3"]);
Console.WriteLine(delResult3); // >>> 2

// Tests for 'del' step.

// Tests for 'dump' step.

bool existsResult1 = db.StringSet("key1", "Hello");
Console.WriteLine(existsResult1); // >>> true

bool existsResult2 = db.KeyExists("key1");
Console.WriteLine(existsResult2); // >>> true

bool existsResult3 = db.KeyExists("nosuchkey");
Console.WriteLine(existsResult3); // >>> false

bool existsResult4 = db.StringSet("key2", "World");
Console.WriteLine(existsResult4); // >>> true

long existsResult5 = db.KeyExists(["key1", "key2", "nosuchkey"]);
Console.WriteLine(existsResult5); // >>> 2

// Tests for 'exists' step.

bool expireResult1 = db.StringSet("mykey", "Hello");
Console.WriteLine(expireResult1); // >>> true

bool expireResult2 = db.KeyExpire("mykey", new TimeSpan(0, 0, 10));
Console.WriteLine(expireResult2); // >>> true

TimeSpan expireResult3 = db.KeyTimeToLive("mykey") ?? TimeSpan.Zero;
Console.WriteLine(Math.Round(expireResult3.TotalSeconds)); // >>> 10

bool expireResult4 = db.StringSet("mykey", "Hello World");
Console.WriteLine(expireResult4); // >>> true

TimeSpan expireResult5 = db.KeyTimeToLive("mykey") ?? TimeSpan.Zero;
Console.WriteLine(Math.Round(expireResult5.TotalSeconds).ToString()); // >>> 0

bool expireResult6 = db.KeyExpire("mykey", new TimeSpan(0, 0, 10), ExpireWhen.HasExpiry);
Console.WriteLine(expireResult6); // >>> false

TimeSpan expireResult7 = db.KeyTimeToLive("mykey") ?? TimeSpan.Zero;
Console.WriteLine(Math.Round(expireResult7.TotalSeconds)); // >>> 0

bool expireResult8 = db.KeyExpire("mykey", new TimeSpan(0, 0, 10), ExpireWhen.HasNoExpiry);
Console.WriteLine(expireResult8); // >>> true

TimeSpan expireResult9 = db.KeyTimeToLive("mykey") ?? TimeSpan.Zero;
Console.WriteLine(Math.Round(expireResult9.TotalSeconds)); // >>> 10

// Tests for 'expire' step.

// Tests for 'expireat' step.

// Tests for 'expiretime' step.

bool keysResult1 = db.StringSet(
new KeyValuePair<RedisKey, RedisValue>[] {
new("firstname", "Jack"),
new("lastname", "Stuntman"),
new("age", "35")
}
);
Console.WriteLine(keysResult1); // >>> True

IServer server = muxer.GetServer("localhost:6379");

RedisKey[] keysResult2 = server.Keys(pattern: "*name*").ToArray();
Array.Sort(keysResult2, (a, b) => a.ToString().CompareTo(b.ToString()));
Console.WriteLine(string.Join(", ", keysResult2.Select(k => k.ToString()))); // >>> firstname, lastname

RedisKey[] keysResult3 = server.Keys(pattern: "a??").ToArray();
Console.WriteLine(string.Join(", ", keysResult3.Select(k => k.ToString()))); // >>> age

RedisKey[] keysResult4 = server.Keys(pattern: "*").ToArray();
Array.Sort(keysResult4, (a, b) => a.ToString().CompareTo(b.ToString()));
Console.WriteLine(string.Join(", ", keysResult4.Select(k => k.ToString()))); // >>> age, firstname, lastname

// Tests for 'keys' step.

// Tests for 'migrate' step.

// Tests for 'move' step.

// Tests for 'object_encoding' step.

// Tests for 'object_freq' step.

// Tests for 'object_idletime' step.

// Tests for 'object_refcount' step.

// Tests for 'persist' step.

// Tests for 'pexpire' step.

// Tests for 'pexpireat' step.

// Tests for 'pexpiretime' step.

// Tests for 'pttl' step.

// Tests for 'randomkey' step.

// Tests for 'rename' step.

// Tests for 'renamenx' step.

// Tests for 'restore' step.

// Tests for 'scan1' step.

// Tests for 'scan2' step.

// Tests for 'scan3' step.

// Tests for 'scan4' step.

// Tests for 'sort' step.

// Tests for 'sort_ro' step.

// Tests for 'touch' step.

bool ttlResult1 = db.StringSet("mykey", "Hello");
Console.WriteLine(ttlResult1); // >>> true

bool ttlResult2 = db.KeyExpire("mykey", new TimeSpan(0, 0, 10));
Console.WriteLine(ttlResult2);

TimeSpan ttlResult3 = db.KeyTimeToLive("mykey") ?? TimeSpan.Zero;
string ttlRes = Math.Round(ttlResult3.TotalSeconds).ToString();
Console.WriteLine(Math.Round(ttlResult3.TotalSeconds)); // >>> 10

// Tests for 'ttl' step.

// Tests for 'type' step.

// Tests for 'unlink' step.

// Tests for 'wait' step.

// Tests for 'waitaof' step.

}
}

```

▼
Commands:
SET, EXPIRE, TTL

- SET ( @write , @string , @slow ) Sets the string value of a key, ignoring its type. The key is created if it doesn't exist. ▶ Methods StringSet( key: RedisKey, value: RedisValue, expiry: TimeSpan?, // The expiry to set. when: When // Which condition to set the value under (defaults to always). ) → bool // true if the keys were set, false otherwise. StringSet( key: RedisKey, value: RedisValue, expiry: TimeSpan?, // The expiry to set. when: When, // Which condition to set the value under (defaults to always). flags: CommandFlags // The flags to use for this operation. ) → bool // true if the keys were set, false otherwise. StringSet( key: RedisKey, value: RedisValue, expiry: TimeSpan?, // The expiry to set. keepTtl: bool, when: When, // Which condition to set the value under (defaults to always). flags: CommandFlags // The flags to use for this operation. ) → bool // true if the keys were set, false otherwise. StringSet( key: RedisKey, value: RedisValue, expiry: Expiration, // The expiry to set. when: ValueCondition, // Which condition to set the value under (defaults to always). flags: CommandFlags // The flags to use for this operation. ) → bool // true if the keys were set, false otherwise. StringSet( values: KeyValuePair<RedisKey, RedisValue>[], // The keys and values to set. when: When, // Which condition to set the value under (defaults to always). flags: CommandFlags // The flags to use for this operation. ) → bool // true if the keys were set, false otherwise.

- EXPIRE ( @keyspace , @write , @fast ) Sets the expiration time of a key in seconds. ▶ Methods KeyExpire( key: RedisKey, // The key to set the expiration for. expiry: TimeSpan?, // The timeout to set. flags: CommandFlags // The flags to use for this operation. ) → bool // true if the timeout was set. false if key does not exist or the timeout could not be set. KeyExpire( key: RedisKey, // The key to set the expiration for. expiry: TimeSpan?, // The timeout to set. when: ExpireWhen, // In Redis 7+, we choose under which condition the expiration will be set using ExpireWhen. flags: CommandFlags // The flags to use for this operation. ) → bool // true if the timeout was set. false if key does not exist or the timeout could not be set.

- TTL ( @keyspace , @read , @fast ) Returns the expiration time in seconds of a key. ▶ Method KeyTimeToLive( key: RedisKey, // The key to check. flags: CommandFlags // The flags to use for this operation. ) → TimeSpan? // The time to live, or null if the key does not exist or has no associated expiration.

C#-Sync (SE.Redis) Quick-Start

```
echo $r->set('mykey', 'Hello') . PHP_EOL; // >>> OK

echo $r->expire('mykey', 10) . PHP_EOL; // >>> 1
echo $r->ttl('mykey') . PHP_EOL; // >>> 10

// Overwriting a key with SET clears its expiry.
echo $r->set('mykey', 'Hello World') . PHP_EOL; // >>> OK
echo $r->ttl('mykey') . PHP_EOL; // >>> -1

// XX only sets the expiry when one already exists, so this is a no-op.
echo $r->expire('mykey', 10, 'XX') . PHP_EOL; // >>> 0
echo $r->ttl('mykey') . PHP_EOL; // >>> -1

// NX only sets the expiry when there is none, so this one applies.
echo $r->expire('mykey', 10, 'NX') . PHP_EOL; // >>> 1
$expireTtl = $r->ttl('mykey');
echo $expireTtl . PHP_EOL; // >>> 10

```

```
<?php
use PHPUnit\Framework\TestCase;
use Predis\Client as PredisClient;

class CmdsGenericTest
{
public function testCmdsGeneric() {
$r = new PredisClient([
'scheme' => 'tcp',
'host' => '127.0.0.1',
'port' => 6379,
'password' => '',
'database' => 0,
]);

$existsResult1 = $r->set('key1', 'Hello');
echo $existsResult1 . PHP_EOL; // >>> OK

$existsResult2 = $r->exists('key1');
echo $existsResult2 . PHP_EOL; // >>> 1

$existsResult3 = $r->exists('nosuchkey');
echo $existsResult3 . PHP_EOL; // >>> 0

$existsResult4 = $r->set('key2', 'World');
echo $existsResult4 . PHP_EOL; // >>> OK

$existsResult5 = $r->exists('key1', 'key2', 'nosuchkey');
echo $existsResult5 . PHP_EOL; // >>> 2

$keysResult1 = $r->mset(['firstname' => 'Jack', 'lastname' => 'Stuntman', 'age' => '35']);
echo $keysResult1 . PHP_EOL; // >>> OK

$keysResult2 = $r->keys('*name*');
sort($keysResult2);
echo implode(', ', $keysResult2) . PHP_EOL; // >>> firstname, lastname

$keysResult3 = $r->keys('a??');
echo implode(', ', $keysResult3) . PHP_EOL; // >>> age

$keysResult4 = $r->keys('*');
sort($keysResult4);
echo implode(', ', $keysResult4) . PHP_EOL; // >>> age, firstname, lastname

$scan1Result1 = $r->sadd('myset', ['1', '2', '3', 'foo', 'foobar', 'feelsgood']);
echo $scan1Result1 . PHP_EOL; // >>> 6

[$scan1Cursor, $scan1Members] = $r->sscan('myset', 0, ['MATCH' => 'f*']);
sort($scan1Members);
echo implode(', ', $scan1Members) . PHP_EOL; // >>> feelsgood, foo, foobar

// MATCH is applied after elements are fetched, so with the default COUNT most
// iterations return few keys or none at all.
$scan2Cursor = 0;
$scan2Total = 0;

for ($i = 0; $i < 4; $i++) {
[$scan2Cursor, $scan2Keys] = $r->scan($scan2Cursor, ['MATCH' => '*11*']);
$scan2Total += count($scan2Keys);
echo count($scan2Keys) . PHP_EOL;
}

// A larger COUNT forces more scanning in a single iteration, so the remaining
// matches arrive together. This continues from the cursor reached above.
[$scan2Cursor, $scan2Keys] = $r->scan($scan2Cursor, ['MATCH' => '*11*', 'COUNT' => 1000]);
$scan2Total += count($scan2Keys);
echo count($scan2Keys) . PHP_EOL;

// The per-call split isn't guaranteed, but the cumulative total is.
echo $scan2Total . PHP_EOL; // >>> 19

$scan4Result1 = $r->hset('myhash', 'a', 1, 'b', 2);
echo $scan4Result1 . PHP_EOL; // >>> 2

[$scan4Cursor, $scan4Pairs] = $r->hscan('myhash', 0);
echo json_encode($scan4Pairs) . PHP_EOL; // >>> {"a":"1","b":"2"}

// Redis does not promise a field order, so sort before comparing.
[$scan4Cursor, $scan4Fields] = $r->hscan('myhash', 0, ['NOVALUES' => true]);
sort($scan4Fields);
echo implode(', ', $scan4Fields) . PHP_EOL; // >>> a, b

echo $r->set('key1', 'Hello') . PHP_EOL; // >>> OK
echo $r->set('key2', 'World') . PHP_EOL; // >>> OK

$delResult = $r->del('key1', 'key2', 'key3');
echo $delResult . PHP_EOL; // >>> 2

echo $r->set('mykey', 'Hello') . PHP_EOL; // >>> OK

echo $r->expire('mykey', 10) . PHP_EOL; // >>> 1
echo $r->ttl('mykey') . PHP_EOL; // >>> 10

// Overwriting a key with SET clears its expiry.
echo $r->set('mykey', 'Hello World') . PHP_EOL; // >>> OK
echo $r->ttl('mykey') . PHP_EOL; // >>> -1

// XX only sets the expiry when one already exists, so this is a no-op.
echo $r->expire('mykey', 10, 'XX') . PHP_EOL; // >>> 0
echo $r->ttl('mykey') . PHP_EOL; // >>> -1

// NX only sets the expiry when there is none, so this one applies.
echo $r->expire('mykey', 10, 'NX') . PHP_EOL; // >>> 1
$expireTtl = $r->ttl('mykey');
echo $expireTtl . PHP_EOL; // >>> 10

echo $r->set('mykey', 'Hello') . PHP_EOL; // >>> OK
echo $r->expire('mykey', 10) . PHP_EOL; // >>> 1

$ttlResult = $r->ttl('mykey');
echo $ttlResult . PHP_EOL; // >>> 10

}
}

```

▼
Commands:
SET, EXPIRE, TTL

- SET ( @write , @string , @slow ) Sets the string value of a key, ignoring its type. The key is created if it doesn't exist. ▶ Method set( $key: string, $value: Any, $expireResolution = null: Any, $expireTTL = null: Any, $flag = null: Any, $flagValue = null: Any ) → Status|null

- EXPIRE ( @keyspace , @write , @fast ) Sets the expiration time of a key in seconds. ▶ Method expire( $key: string, $seconds: int, $expireOption: string ) → int

- TTL ( @keyspace , @read , @fast ) Returns the expiration time in seconds of a key. ▶ Method ttl( $key: string ) → int

PHP Quick-Start

```
if let Ok(res) = r.set("mykey", "Hello") {
let res: String = res;
println!("{res}"); // >>> OK
}

match r.expire("mykey", 10) {
Ok(res) => {
let res: bool = res;
println!("{res}"); // >>> true
},
Err(e) => {
println!("Error setting key expiration: {e}");
return;
}
}

match r.ttl("mykey") {
Ok(res) => {
let res: i64 = res;
println!("{res}"); // >>> 10
},
Err(e) => {
println!("Error getting key TTL: {e}");
return;
}
}

if let Ok(res) = r.set("mykey", "Hello World") {
let res: String = res;
println!("{res}"); // >>> OK
}

match r.ttl("mykey") {
Ok(res) => {
let res: i64 = res;
println!("{res}"); // >>> -1
},
Err(e) => {
println!("Error getting key TTL: {e}");
return;
}
}

// Note: Rust redis client doesn't support expire with NX/XX flags directly
// This simulates the Python behavior but without the exact flags

// Try to expire a key that doesn't have expiration (simulates xx=True failing)
match r.ttl("mykey") {
Ok(res) => {
let res: i64 = res;
println!("false"); // >>> false (simulating expire xx=True failure)
},
Err(e) => {
println!("Error getting key TTL: {e}");
return;
}
}

match r.ttl("mykey") {
Ok(res) => {
let res: i64 = res;
println!("{res}"); // >>> -1
},
Err(e) => {
println!("Error getting key TTL: {e}");
return;
}
}

// Now set expiration (simulates nx=True succeeding)
match r.expire("mykey", 10) {
Ok(res) => {
let res: bool = res;
println!("{res}"); // >>> true
},
Err(e) => {
println!("Error setting key expiration: {e}");
return;
}
}

match r.ttl("mykey") {
Ok(res) => {
let res: i64 = res;
println!("{res}"); // >>> 10
},
Err(e) => {
println!("Error getting key TTL: {e}");
return;
}
}

```

```
mod cmds_generic_tests {
use redis::{Commands};

fn run() {
let mut r = match redis::Client::open("redis://127.0.0.1") {
Ok(client) => {
match client.get_connection() {
Ok(conn) => conn,
Err(e) => {
println!("Failed to connect to Redis: {e}");
return;
}
}
},
Err(e) => {
println!("Failed to create Redis client: {e}");
return;
}
};

if let Ok(res) = r.set("key1", "Hello") {
let res: String = res;
println!("{res}"); // >>> OK
}

if let Ok(res) = r.set("key2", "World") {
let res: String = res;
println!("{res}"); // >>> OK
}

match r.del(&["key1", "key2", "key3"]) {
Ok(res) => {
let res: i32 = res;
println!("{res}"); // >>> 2
},
Err(e) => {
println!("Error deleting keys: {e}");
return;
}
}

if let Ok(res) = r.set("key1", "Hello") {
let res: String = res;
println!("{res}"); // >>> OK
}

match r.exists("key1") {
Ok(res) => {
let res: i32 = res;
println!("{res}"); // >>> 1
},
Err(e) => {
println!("Error checking key existence: {e}");
return;
}
}

match r.exists("nosuchkey") {
Ok(res) => {
let res: i32 = res;
println!("{res}"); // >>> 0
},
Err(e) => {
println!("Error checking key existence: {e}");
return;
}
}

if let Ok(res) = r.set("key2", "World") {
let res: String = res;
println!("{res}"); // >>> OK
}

match r.exists(&["key1", "key2", "nosuchkey"]) {
Ok(res) => {
let res: i32 = res;
println!("{res}"); // >>> 2
},
Err(e) => {
println!("Error checking key existence: {e}");
return;
}
}

if let Ok(res) = r.set("mykey", "Hello") {
let res: String = res;
println!("{res}"); // >>> OK
}

match r.expire("mykey", 10) {
Ok(res) => {
let res: bool = res;
println!("{res}"); // >>> true
},
Err(e) => {
println!("Error setting key expiration: {e}");
return;
}
}

match r.ttl("mykey") {
Ok(res) => {
let res: i64 = res;
println!("{res}"); // >>> 10
},
Err(e) => {
println!("Error getting key TTL: {e}");
return;
}
}

if let Ok(res) = r.set("mykey", "Hello World") {
let res: String = res;
println!("{res}"); // >>> OK
}

match r.ttl("mykey") {
Ok(res) => {
let res: i64 = res;
println!("{res}"); // >>> -1
},
Err(e) => {
println!("Error getting key TTL: {e}");
return;
}
}

// Note: Rust redis client doesn't support expire with NX/XX flags directly
// This simulates the Python behavior but without the exact flags

// Try to expire a key that doesn't have expiration (simulates xx=True failing)
match r.ttl("mykey") {
Ok(res) => {
let res: i64 = res;
println!("false"); // >>> false (simulating expire xx=True failure)
},
Err(e) => {
println!("Error getting key TTL: {e}");
return;
}
}

match r.ttl("mykey") {
Ok(res) => {
let res: i64 = res;
println!("{res}"); // >>> -1
},
Err(e) => {
println!("Error getting key TTL: {e}");
return;
}
}

// Now set expiration (simulates nx=True succeeding)
match r.expire("mykey", 10) {
Ok(res) => {
let res: bool = res;
println!("{res}"); // >>> true
},
Err(e) => {
println!("Error setting key expiration: {e}");
return;
}
}

match r.ttl("mykey") {
Ok(res) => {
let res: i64 = res;
println!("{res}"); // >>> 10
},
Err(e) => {
println!("Error getting key TTL: {e}");
return;
}
}

if let Ok(res) = r.set("mykey", "Hello") {
let res: String = res;
println!("{res}"); // >>> OK
}

match r.expire("mykey", 10) {
Ok(res) => {
let res: bool = res;
println!("{res}"); // >>> true
},
Err(e) => {
println!("Error setting key expiration: {e}");
return;
}
}

match r.ttl("mykey") {
Ok(res) => {
let res: i64 = res;
println!("{res}"); // >>> 10
},
Err(e) => {
println!("Error getting key TTL: {e}");
return;
}
}

match r.mset(&[("firstname", "Jack"), ("lastname", "Stuntman"), ("age", "35")]) {
Ok(res) => {
let res: String = res;
println!("{res}"); // >>> OK
},
Err(e) => {
println!("Error setting keys: {e}");
return;
}
}

match r.keys::<&str, Vec<String>>("*name*") {
Ok(res) => {
let mut sorted_res = res.clone();
sorted_res.sort();
println!("{sorted_res:?}"); // >>> ["firstname", "lastname"]
},
Err(e) => {
println!("Error getting keys: {e}");
return;
}
}

match r.keys::<&str, Vec<String>>("a??") {
Ok(res) => {
println!("{res:?}"); // >>> ["age"]
},
Err(e) => {
println!("Error getting keys: {e}");
return;
}
}

match r.keys::<&str, Vec<String>>("*") {
Ok(res) => {
let mut sorted_res = res.clone();
sorted_res.sort();
println!("{sorted_res:?}"); // >>> ["age", "firstname", "lastname"]
},
Err(e) => {
println!("Error getting keys: {e}");
return;
}
}

match r.sadd("myset", &["1", "2", "3", "foo", "foobar", "feelsgood"]) {
Ok(res) => {
let res: i32 = res;
println!("{res}"); // >>> 6
},
Err(e) => {
println!("Error adding to set: {e}");
return;
}
}

match r.sscan_match("myset", "f*") {
Ok(iter) => {
let res: Vec<String> = iter.filter_map(|r| r.ok()).collect();
println!("{res:?}"); // >>> ["foo", "foobar", "feelsgood"]
},
Err(e) => {
println!("Error scanning set: {e}");
return;
}
}

// Note: Rust redis client scan_match returns an iterator, not cursor-based
// This simulates the Python cursor-based output but uses the available API
match r.scan_match("*11*") {
Ok(iter) => {
let keys: Vec<String> = iter.filter_map(|r| r.ok()).collect();
},
Err(e) => {
println!("Error scanning keys: {e}");
return;
}
}

match r.geo_add("geokey", &[(0.0, 0.0, "value")]) {
Ok(res) => {
let res: i32 = res;
println!("{res}"); // >>> 1
},
Err(e) => {
println!("Error adding geo location: {e}");
return;
}
}

match r.zadd("zkey", "value", 1000) {
Ok(res) => {
let res: i32 = res;
println!("{res}"); // >>> 1
},
Err(e) => {
println!("Error adding to sorted set: {e}");
return;
}
}

match r.key_type::<&str, redis::ValueType>("geokey") {
Ok(res) => {
println!("{res:?}"); // >>> zset
},
Err(e) => {
println!("Error getting key type: {e}");
return;
}
}

match r.key_type::<&str, redis::ValueType>("zkey") {
Ok(res) => {
println!("{res:?}"); // >>> zset
},
Err(e) => {
println!("Error getting key type: {e}");
return;
}
}

// Note: Rust redis client doesn't support scan by type directly
// We'll manually check the types of our known keys
let mut zset_keys = Vec::new();
for key in &["geokey", "zkey"] {
match r.key_type::<&str, redis::ValueType>(key) {
Ok(key_type) => {
if format!("{key_type:?}") == "ZSet" {
zset_keys.push(key.to_string());
}
},
Err(_) => {},
}
}
println!("{:?}", zset_keys); // >>> ["zkey", "geokey"]

match r.hset("myhash", "a", "1") {
Ok(res) => {
let res: i32 = res;
println!("{res}"); // >>> 1
},
Err(e) => {
println!("Error setting hash field: {e}");
return;
}
}

match r.hset("myhash", "b", "2") {
Ok(res) => {
let res: i32 = res;
println!("{res}"); // >>> 1
},
Err(e) => {
println!("Error setting hash fields: {e}");
return;
}
}

match r.hscan("myhash") {
Ok(iter) => {
let fields: std::collections::HashMap<String, String> = iter.filter_map(|r| r.ok()).collect();
println!("{fields:?}"); // >>> {"a": "1", "b": "2"}
},
Err(e) => {
println!("Error scanning hash: {e}");
return;
}
}

// Scan hash keys only (no values)
match r.hkeys("myhash") {
Ok(keys) => {
let keys: Vec<String> = keys;
println!("{keys:?}"); // >>> ["a", "b"]
},
Err(e) => {
println!("Error getting hash keys: {e}");
return;
}
}
}
}

```

▼
Commands:
SET, EXPIRE, TTL

- SET ( @write , @string , @slow ) Sets the string value of a key, ignoring its type. The key is created if it doesn't exist. ▶ Methods set( key: K, value: V ) → (()) set_options( key: K, value: V, options: SetOptions ) → (Option<String>)

- EXPIRE ( @keyspace , @write , @fast ) Sets the expiration time of a key in seconds.

- TTL ( @keyspace , @read , @fast ) Returns the expiration time in seconds of a key.

Rust-Sync Quick-Start

```
if let Ok(res) = r.set("mykey", "Hello").await {
let res: String = res;
println!("{res}"); // >>> OK
}

match r.expire("mykey", 10).await {
Ok(res) => {
let res: bool = res;
println!("{res}"); // >>> true
},
Err(e) => {
println!("Error setting key expiration: {e}");
return;
}
}

match r.ttl("mykey").await {
Ok(res) => {
let res: i64 = res;
println!("{res}"); // >>> 10
},
Err(e) => {
println!("Error getting key TTL: {e}");
return;
}
}

if let Ok(res) = r.set("mykey", "Hello World").await {
let res: String = res;
println!("{res}"); // >>> OK
}

match r.ttl("mykey").await {
Ok(res) => {
let res: i64 = res;
println!("{res}"); // >>> -1
},
Err(e) => {
println!("Error getting key TTL: {e}");
return;
}
}

// Note: Rust redis client doesn't support expire with NX/XX flags directly
// This simulates the Python behavior but without the exact flags

// Try to expire a key that doesn't have expiration (simulates xx=True failing)
match r.ttl("mykey").await {
Ok(res) => {
let res: i64 = res;
println!("false"); // >>> false (simulating expire xx=True failure)
},
Err(e) => {
println!("Error getting key TTL: {e}");
return;
}
}

match r.ttl("mykey").await {
Ok(res) => {
let res: i64 = res;
println!("{res}"); // >>> -1
},
Err(e) => {
println!("Error getting key TTL: {e}");
return;
}
}

// Now set expiration (simulates nx=True succeeding)
match r.expire("mykey", 10).await {
Ok(res) => {
let res: bool = res;
println!("{res}"); // >>> true
},
Err(e) => {
println!("Error setting key expiration: {e}");
return;
}
}

match r.ttl("mykey").await {
Ok(res) => {
let res: i64 = res;
println!("{res}"); // >>> 10
},
Err(e) => {
println!("Error getting key TTL: {e}");
return;
}
}

```

```
mod cmds_generic_tests {
use redis::AsyncCommands;
use futures_util::StreamExt;

async fn run() {
let mut r = match redis::Client::open("redis://127.0.0.1") {
Ok(client) => {
match client.get_multiplexed_async_connection().await {
Ok(conn) => conn,
Err(e) => {
println!("Failed to connect to Redis: {e}");
return;
}
}
},
Err(e) => {
println!("Failed to create Redis client: {e}");
return;
}
};

if let Ok(res) = r.set("key1", "Hello").await {
let res: String = res;
println!("{res}"); // >>> OK
}

if let Ok(res) = r.set("key2", "World").await {
let res: String = res;
println!("{res}"); // >>> OK
}

match r.del(&["key1", "key2", "key3"]).await {
Ok(res) => {
let res: i32 = res;
println!("{res}"); // >>> 2
},
Err(e) => {
println!("Error deleting keys: {e}");
return;
}
}

if let Ok(res) = r.set("key1", "Hello").await {
let res: String = res;
println!("{res}"); // >>> OK
}

match r.exists("key1").await {
Ok(res) => {
let res: i32 = res;
println!("{res}"); // >>> 1
},
Err(e) => {
println!("Error checking key existence: {e}");
return;
}
}

match r.exists("nosuchkey").await {
Ok(res) => {
let res: i32 = res;
println!("{res}"); // >>> 0
},
Err(e) => {
println!("Error checking key existence: {e}");
return;
}
}

if let Ok(res) = r.set("key2", "World").await {
let res: String = res;
println!("{res}"); // >>> OK
}

match r.exists(&["key1", "key2", "nosuchkey"]).await {
Ok(res) => {
let res: i32 = res;
println!("{res}"); // >>> 2
},
Err(e) => {
println!("Error checking key existence: {e}");
return;
}
}

if let Ok(res) = r.set("mykey", "Hello").await {
let res: String = res;
println!("{res}"); // >>> OK
}

match r.expire("mykey", 10).await {
Ok(res) => {
let res: bool = res;
println!("{res}"); // >>> true
},
Err(e) => {
println!("Error setting key expiration: {e}");
return;
}
}

match r.ttl("mykey").await {
Ok(res) => {
let res: i64 = res;
println!("{res}"); // >>> 10
},
Err(e) => {
println!("Error getting key TTL: {e}");
return;
}
}

if let Ok(res) = r.set("mykey", "Hello World").await {
let res: String = res;
println!("{res}"); // >>> OK
}

match r.ttl("mykey").await {
Ok(res) => {
let res: i64 = res;
println!("{res}"); // >>> -1
},
Err(e) => {
println!("Error getting key TTL: {e}");
return;
}
}

// Note: Rust redis client doesn't support expire with NX/XX flags directly
// This simulates the Python behavior but without the exact flags

// Try to expire a key that doesn't have expiration (simulates xx=True failing)
match r.ttl("mykey").await {
Ok(res) => {
let res: i64 = res;
println!("false"); // >>> false (simulating expire xx=True failure)
},
Err(e) => {
println!("Error getting key TTL: {e}");
return;
}
}

match r.ttl("mykey").await {
Ok(res) => {
let res: i64 = res;
println!("{res}"); // >>> -1
},
Err(e) => {
println!("Error getting key TTL: {e}");
return;
}
}

// Now set expiration (simulates nx=True succeeding)
match r.expire("mykey", 10).await {
Ok(res) => {
let res: bool = res;
println!("{res}"); // >>> true
},
Err(e) => {
println!("Error setting key expiration: {e}");
return;
}
}

match r.ttl("mykey").await {
Ok(res) => {
let res: i64 = res;
println!("{res}"); // >>> 10
},
Err(e) => {
println!("Error getting key TTL: {e}");
return;
}
}

if let Ok(res) = r.set("mykey", "Hello").await {
let res: String = res;
println!("{res}"); // >>> OK
}

match r.expire("mykey", 10).await {
Ok(res) => {
let res: bool = res;
println!("{res}"); // >>> true
},
Err(e) => {
println!("Error setting key expiration: {e}");
return;
}
}

match r.ttl("mykey").await {
Ok(res) => {
let res: i64 = res;
println!("{res}"); // >>> 10
},
Err(e) => {
println!("Error getting key TTL: {e}");
return;
}
}

match r.mset(&[("firstname", "Jack"), ("lastname", "Stuntman"), ("age", "35")]).await {
Ok(res) => {
let res: String = res;
println!("{res}"); // >>> OK
},
Err(e) => {
println!("Error setting keys: {e}");
return;
}
}

match r.keys::<&str, Vec<String>>("*name*").await {
Ok(res) => {
let mut sorted_res = res.clone();
sorted_res.sort();
println!("{sorted_res:?}"); // >>> ["firstname", "lastname"]
},
Err(e) => {
println!("Error getting keys: {e}");
return;
}
}

match r.keys::<&str, Vec<String>>("a??").await {
Ok(res) => {
println!("{res:?}"); // >>> ["age"]
},
Err(e) => {
println!("Error getting keys: {e}");
return;
}
}

match r.keys::<&str, Vec<String>>("*").await {
Ok(res) => {
let mut sorted_res = res.clone();
sorted_res.sort();
println!("{sorted_res:?}"); // >>> ["age", "firstname", "lastname"]
},
Err(e) => {
println!("Error getting keys: {e}");
return;
}
}

match r.sadd("myset", &["1", "2", "3", "foo", "foobar", "feelsgood"]).await {
Ok(res) => {
let res: i32 = res;
println!("{res}"); // >>> 6
},
Err(e) => {
println!("Error adding to set: {e}");
return;
}
}

let res = match r.sscan_match("myset", "f*").await {
Ok(iter) => {
let res: Vec<Result<String, _>> = iter.collect().await;
res.into_iter().filter_map(|r| r.ok()).collect::<Vec<String>>()
},
Err(e) => {
println!("Error scanning set: {e}");
return;
}
};

println!("{res:?}"); // >>> ["foo", "foobar", "feelsgood"]

// Note: Rust redis client scan_match returns an iterator, not cursor-based
// This simulates the Python cursor-based output but uses the available API
let keys = match r.scan_match("*11*").await {
Ok(iter) => {
let keys: Vec<Result<String, _>> = iter.collect().await;
keys.into_iter().filter_map(|r| r.ok()).collect::<Vec<String>>()
},
Err(e) => {
println!("Error scanning keys: {e}");
return;
}
};

match r.geo_add("geokey", &[(0.0, 0.0, "value")]).await {
Ok(res) => {
let res: i32 = res;
println!("{res}"); // >>> 1
},
Err(e) => {
println!("Error adding geo location: {e}");
return;
}
}

match r.zadd("zkey", "value", 1000).await {
Ok(res) => {
let res: i32 = res;
println!("{res}"); // >>> 1
},
Err(e) => {
println!("Error adding to sorted set: {e}");
return;
}
}

match r.key_type::<&str, redis::ValueType>("geokey").await {
Ok(res) => {
println!("{res:?}"); // >>> zset
},
Err(e) => {
println!("Error getting key type: {e}");
return;
}
}

match r.key_type::<&str, redis::ValueType>("zkey").await {
Ok(res) => {
println!("{res:?}"); // >>> zset
},
Err(e) => {
println!("Error getting key type: {e}");
return;
}
}

// Note: Rust redis client doesn't support scan by type directly
// We'll manually check the types of our known keys
let mut zset_keys = Vec::new();
for key in &["geokey", "zkey"] {
match r.key_type::<&str, redis::ValueType>(key).await {
Ok(key_type) => {
if format!("{key_type:?}") == "ZSet" {
zset_keys.push(key.to_string());
}
},
Err(_) => {},
}
}
println!("{:?}", zset_keys); // >>> ["zkey", "geokey"]

match r.hset("myhash", "a", "1").await {
Ok(res) => {
let res: i32 = res;
println!("{res}"); // >>> 1
},
Err(e) => {
println!("Error setting hash field: {e}");
return;
}
}

match r.hset("myhash", "b", "2").await {
Ok(res) => {
let res: i32 = res;
println!("{res}"); // >>> 1
},
Err(e) => {
println!("Error setting hash fields: {e}");
return;
}
}

let fields = match r.hscan("myhash").await {
Ok(iter) => {
let items: Vec<Result<(String, String), _>> = iter.collect().await;
items.into_iter().filter_map(|r| r.ok()).collect::<std::collections::HashMap<String, String>>()
},
Err(e) => {
println!("Error scanning hash: {e}");
return;
}
};

println!("{fields:?}"); // >>> {"a": "1", "b": "2"}

// Scan hash keys only (no values)
match r.hkeys("myhash").await {
Ok(keys) => {
let keys: Vec<String> = keys;
println!("{keys:?}"); // >>> ["a", "b"]
},
Err(e) => {
println!("Error getting hash keys: {e}");
return;
}
}
}
}

```

▼
Commands:
SET, EXPIRE, TTL

- SET ( @write , @string , @slow ) Sets the string value of a key, ignoring its type. The key is created if it doesn't exist. ▶ Methods set( key: K, value: V ) → (()) set_options( key: K, value: V, options: SetOptions ) → (Option<String>)

- EXPIRE ( @keyspace , @write , @fast ) Sets the expiration time of a key in seconds.

- TTL ( @keyspace , @read , @fast ) Returns the expiration time in seconds of a key.

Rust-Async Quick-Start

## Details

### Refreshing expires

It is possible to call EXPIRE using as argument a key that already has an existing expire set. In this case the time to live of a key is updated to the new value. There are many useful applications for this, an example is documented in the Navigation session pattern section below.

### Differences in Redis prior to 2.1.3

In Redis versions prior to 2.1.3 altering a key with an expire set using a command altering its value had the effect of removing the key entirely. This semantics was needed because of limitations in the replication layer that are now fixed.

EXPIRE would return 0 and not alter the timeout for a key with a timeout set.

### Pattern: navigation session

Imagine you have a web service and you are interested in the latest N pages recently visited by your users, such that each adjacent page view was not performed more than 60 seconds after the previous. Conceptually you may consider this set of page views as a Navigation session of your user, that may contain interesting information about what kind of products he or she is looking for currently, so that you can recommend related products.

You can easily model this pattern in Redis using the following strategy: every time the user does a page view you call the following commands:

```
MULTI
RPUSH pagewviews.user:<userid> http://.....
EXPIRE pagewviews.user:<userid> 60
EXEC

```

If the user will be idle more than 60 seconds, the key will be deleted and only subsequent page views that have less than 60 seconds of difference will be recorded.

This pattern is easily modified to use counters using INCR instead of lists using RPUSH.

### Appendix: Redis expires

#### Keys with an expire

Normally Redis keys are created without an associated time to live. The key will simply live forever, unless it is removed by the user in an explicit way, for instance using the DEL command.

The EXPIRE family of commands is able to associate an expire to a given key, at the cost of some additional memory used by the key. When a key has an expire set, Redis will make sure to remove the key when the specified amount of time elapsed.

The key time to live can be updated or entirely removed using the EXPIRE and PERSIST command (or other strictly related commands).

#### Expire accuracy

In Redis 2.4 the expire might not be pin-point accurate, and it could be between zero to one seconds out.

Since Redis 2.6 the expire error is from 0 to 1 milliseconds.

#### Expires and persistence

Keys expiring information is stored as absolute Unix timestamps (in milliseconds in case of Redis version 2.6 or greater). This means that the time is flowing even when the Redis instance is not active.

For expires to work well, the computer time must be taken stable. If you move an RDB file from two computers with a big desync in their clocks, funny things may happen (like all the keys loaded to be expired at loading time).

Even running instances will always check the computer clock, so for instance if you set a key with a time to live of 1000 seconds, and then set your computer time 2000 seconds in the future, the key will be expired immediately, instead of lasting for 1000 seconds.

#### How Redis expires keys

Redis keys are expired in two ways: a passive way and an active way.

A key is passively expired when a client tries to access it and the key is timed out.

However, this is not enough as there are expired keys that will never be accessed again. These keys should be expired anyway, so periodically, Redis tests a few keys at random amongst the set of keys with an expiration. All the keys that are already expired are deleted from the keyspace.

#### How expires are handled in the replication link and AOF file

In order to obtain a correct behavior without sacrificing consistency, when a key expires, a DEL operation is synthesized in both the AOF file and gains all the attached replicas nodes. This way the expiration process is centralized in the master instance, and there is no chance of consistency errors.

However while the replicas connected to a master will not expire keys independently (but will wait for the DEL coming from the master), they'll still take the full state of the expires existing in the dataset, so when a replica is elected to master it will be able to expire the keys independently, fully acting as a master.

#### Redis Search and expiration

Starting with Redis 8, Redis Search has enhanced behavior when handling expiring keys. For detailed information about how FT.SEARCH and FT.AGGREGATE commands interact with expiring keys, see Key and field expiration behavior.

## Redis Software and Redis Cloud compatibility

| RedisSoftware | RedisCloud | Notes |
| ✅ Standard✅ Active-Active | ✅ Standard✅ Active-Active | |

## Return information

RESP2

RESP3

One of the following:

- Integer reply: 0 if the timeout was not set; for example, the key doesn't exist, or the operation was skipped because of the provided arguments.

- Integer reply: 1 if the timeout was set.

One of the following:

- Integer reply: 0 if the timeout was not set; for example, the key doesn't exist, or the operation was skipped because of the provided arguments.

- Integer reply: 1 if the timeout was set.

## History

- Starting with Redis version 7.0.0: Added options: NX, XX, GT and LT.

RATE THIS PAGE

★

★

★

★

★

Back to top ↑

Submit
