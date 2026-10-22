Skip to content

Node.js

- About this documentation

- Usage and example

- Assertion testing

- Asynchronous context tracking

- Async hooks

- Buffer

- C++ addons

- C/C++ addons with Node-API

- C++ embedder API

- Child processes

- Cluster

- Command-line options

- Console

- Crypto

- Debugger

- Deprecated APIs

- Diagnostics Channel

- DNS

- Domain

- Errors

- Events

- File system

- Globals

- HTTP

- HTTP/2

- HTTPS

- Inspector

- Internationalization

- Modules: CommonJS modules

- Modules: ECMAScript modules

- Modules: node:module API

- Modules: Packages

- Modules: TypeScript

- Net

- OS

- Path

- Performance hooks

- Permissions

- Process

- Punycode

- Query strings

- Readline

- REPL

- Report

- Single executable applications

- SQLite

- Stream

- String decoder

- Test runner

- Timers

- TLS/SSL

- Trace events

- TTY

- UDP/datagram

- URL

- Utilities

- V8

- VM

- WASI

- Web Crypto API

- Web Streams API

- Worker threads

- Zlib

- Code repository and issue tracker

Table of contents

- Modules: CommonJS modules Enabling Accessing the main module Package manager tips Loading ECMAScript modules using require() All together Caching Module caching caveats Built-in modules Built-in modules with mandatory node: prefix Cycles File modules Folders as modules Loading from node_modules folders Loading from the global folders The module wrapper The module scope __dirname __filename exports module require(id) require.cache require.extensions require.main require.resolve(request[, options]) require.resolve.paths(request) The module object module.children module.exports exports shortcut module.filename module.id module.isPreloading module.loaded module.parent module.path module.paths module.require(id) The Module object Source map v3 support

## Modules: CommonJS modules#

Stability: 2 - Stable

CommonJS modules are the original way to package JavaScript code for Node.js. Node.js also supports the ECMAScript modules standard used by browsers and other JavaScript runtimes.

In Node.js, each file is treated as a separate module. For example, consider a file named foo.js:

```
const circle = require('./circle.js');
console.log(`The area of a circle of radius 4 is ${circle.area(4)}`); copy
```

On the first line, foo.js loads the module circle.js that is in the same directory as foo.js.

Here are the contents of circle.js:

```
const { PI } = Math;

exports.area = (r) => PI * r ** 2;

exports.circumference = (r) => 2 * PI * r; copy
```

The module circle.js has exported the functions area() and circumference(). Functions and objects are added to the root of a module by specifying additional properties on the special exports object.

Variables local to the module will be private, because the module is wrapped in a function by Node.js (see module wrapper). In this example, the variable PI is private to circle.js.

The module.exports property can be assigned a new value (such as a function or object).

In the following code, bar.js makes use of the square module, which exports a Square class:

```
const Square = require('./square.js');
const mySquare = new Square(2);
console.log(`The area of mySquare is ${mySquare.area()}`); copy
```

The square module is defined in square.js:

```
// Assigning to exports will not modify module, must use module.exports
module.exports = class Square {
constructor(width) {
this.width = width;
}

area() {
return this.width ** 2;
}
}; copy
```

The CommonJS module system is implemented in the module core module.

### Enabling#

Node.js has two module systems: CommonJS modules and ECMAScript modules.

By default, Node.js will treat the following as CommonJS modules:

- Files with a .cjs extension;

- Files with a .js extension when the nearest parent package.json file contains a top-level field "type" with a value of "commonjs".

- Files with a .js extension or without an extension, when the nearest parent package.json file doesn't contain a top-level field "type" or there is no package.json in any parent folder; unless the file contains syntax that errors unless it is evaluated as an ES module. Package authors should include the "type" field, even in packages where all sources are CommonJS. Being explicit about the type of the package will make things easier for build tools and loaders to determine how the files in the package should be interpreted.

- Files with an extension that is not .mjs, .cjs, .json, .node, or .js (when the nearest parent package.json file contains a top-level field "type" with a value of "module", those files will be recognized as CommonJS modules only if they are being included via require(), not when used as the command-line entry point of the program).

See Determining module system for more details.

Calling require() always use the CommonJS module loader. Calling import() always use the ECMAScript module loader.

### Accessing the main module#

When a file is run directly from Node.js, require.main is set to its module. That means that it is possible to determine whether a file has been run directly by testing require.main === module.

For a file foo.js, this will be true if run via node foo.js, but false if run by require('./foo').

When the entry point is not a CommonJS module, require.main is undefined, and the main module is out of reach.

### Package manager tips#

The semantics of the Node.js require() function were designed to be general enough to support reasonable directory structures. Package manager programs such as dpkg, rpm, and npm will hopefully find it possible to build native packages from Node.js modules without modification.

In the following, we give a suggested directory structure that could work:

Let's say that we wanted to have the folder at /usr/lib/node/<some-package>/<some-version> hold the contents of a specific version of a package.

Packages can depend on one another. In order to install package foo, it may be necessary to install a specific version of package bar. The bar package may itself have dependencies, and in some cases, these may even collide or form cyclic dependencies.

Because Node.js looks up the realpath of any modules it loads (that is, it resolves symlinks) and then looks for their dependencies in node_modules folders, this situation can be resolved with the following architecture:

- /usr/lib/node/foo/1.2.3/: Contents of the foo package, version 1.2.3.

- /usr/lib/node/bar/4.3.2/: Contents of the bar package that foo depends on.

- /usr/lib/node/foo/1.2.3/node_modules/bar: Symbolic link to /usr/lib/node/bar/4.3.2/.

- /usr/lib/node/bar/4.3.2/node_modules/*: Symbolic links to the packages that bar depends on.

Thus, even if a cycle is encountered, or if there are dependency conflicts, every module will be able to get a version of its dependency that it can use.

When the code in the foo package does require('bar'), it will get the version that is symlinked into /usr/lib/node/foo/1.2.3/node_modules/bar. Then, when the code in the bar package calls require('quux'), it'll get the version that is symlinked into /usr/lib/node/bar/4.3.2/node_modules/quux.

Furthermore, to make the module lookup process even more optimal, rather than putting packages directly in /usr/lib/node, we could put them in /usr/lib/node_modules/<name>/<version>. Then Node.js will not bother looking for missing dependencies in /usr/node_modules or /node_modules.

In order to make modules available to the Node.js REPL, it might be useful to also add the /usr/lib/node_modules folder to the $NODE_PATH environment variable. Since the module lookups using node_modules folders are all relative, and based on the real path of the files making the calls to require(), the packages themselves can be anywhere.

### Loading ECMAScript modules using require()#

History

| Version | Changes |
| v22.13.0 | This feature no longer emits an experimental warning by default, though the warning can still be emitted by --trace-require-module. |
| v23.0.0, v22.12.0 | This feature is no longer behind the --experimental-require-module CLI flag. |
| v22.12.0 | Support 'module.exports' interop export in require(esm). |
| v22.0.0 | Added in: v22.0.0 |

Stability: 1.2 - Release candidate

The .mjs extension is reserved for ECMAScript Modules. See Determining module system section for more info regarding which files are parsed as ECMAScript modules.

require() only supports loading ECMAScript modules that meet the following requirements:

- The module is fully synchronous (contains no top-level await); and

- One of these conditions are met: The file has a .mjs extension. The file has a .js extension, and the closest package.json contains "type": "module" The file has a .js extension, the closest package.json does not contain "type": "commonjs", and the module contains ES module syntax.

If the ES Module being loaded meets the requirements, require() can load it and return the module namespace object. In this case it is similar to dynamic import() but is run synchronously and returns the name space object directly.

With the following ES Modules:

```
// distance.mjs
export function distance(a, b) { return Math.sqrt((b.x - a.x) ** 2 + (b.y - a.y) ** 2); } copy
```

```
// point.mjs
export default class Point {
constructor(x, y) { this.x = x; this.y = y; }
} copy
```

A CommonJS module can load them with require():

```
const distance = require('./distance.mjs');
console.log(distance);
// [Module: null prototype] {
// distance: [Function: distance]
// }

const point = require('./point.mjs');
console.log(point);
// [Module: null prototype] {
// default: [class Point],
// __esModule: true,
// } copy
```

For interoperability with existing tools that convert ES Modules into CommonJS, which could then load real ES Modules through require(), the returned namespace would contain a __esModule: true property if it has a default export so that consuming code generated by tools can recognize the default exports in real ES Modules. If the namespace already defines __esModule, this would not be added. This property is experimental and can change in the future. It should only be used by tools converting ES modules into CommonJS modules, following existing ecosystem conventions. Code authored directly in CommonJS should avoid depending on it.

When an ES Module contains both named exports and a default export, the result returned by require() is the module namespace object, which places the default export in the .default property, similar to the results returned by import(). To customize what should be returned by require(esm) directly, the ES Module can export the desired value using the string name "module.exports".

```
// point.mjs
export default class Point {
constructor(x, y) { this.x = x; this.y = y; }
}

// `distance` is lost to CommonJS consumers of this module, unless it's
// added to `Point` as a static property.
export function distance(a, b) { return Math.sqrt((b.x - a.x) ** 2 + (b.y - a.y) ** 2); }
export { Point as 'module.exports' } copy
```

```
const Point = require('./point.mjs');
console.log(Point); // [class Point]

// Named exports are lost when 'module.exports' is used
const { distance } = require('./point.mjs');
console.log(distance); // undefined copy
```

Notice in the example above, when the module.exports export name is used, named exports will be lost to CommonJS consumers. To allow CommonJS consumers to continue accessing named exports, the module can make sure that the default export is an object with the named exports attached to it as properties. For example with the example above, distance can be attached to the default export, the Point class, as a static method.

```
export function distance(a, b) { return Math.sqrt((b.x - a.x) ** 2 + (b.y - a.y) ** 2); }

export default class Point {
constructor(x, y) { this.x = x; this.y = y; }
static distance = distance;
}

export { Point as 'module.exports' } copy
```

```
const Point = require('./point.mjs');
console.log(Point); // [class Point]

const { distance } = require('./point.mjs');
console.log(distance); // [Function: distance] copy
```

If the module being require()'d contains top-level await, or the module graph it imports contains top-level await, ERR_REQUIRE_ASYNC_MODULE will be thrown. In this case, users should load the asynchronous module using import().

If --experimental-print-required-tla is enabled, instead of throwing ERR_REQUIRE_ASYNC_MODULE before evaluation, Node.js will evaluate the module, try to locate the top-level awaits, and print their location to help users fix them.

Support for loading ES modules using require() is currently experimental and can be disabled using --no-experimental-require-module. To print where this feature is used, use --trace-require-module.

This feature can be detected by checking if process.features.require_module is true.

### All together#

To get the exact filename that will be loaded when require() is called, use the require.resolve() function.

Putting together all of the above, here is the high-level algorithm in pseudocode of what require() does:

```
require(X) from module at path Y
1. If X is a core module,
a. return the core module
b. STOP
2. If X begins with '/'
a. set Y to the file system root
3. If X is equal to '.', or X begins with './', '/' or '../'
a. LOAD_AS_FILE(Y + X)
b. LOAD_AS_DIRECTORY(Y + X)
c. THROW "not found"
4. If X begins with '#'
a. LOAD_PACKAGE_IMPORTS(X, dirname(Y))
5. LOAD_PACKAGE_SELF(X, dirname(Y))
6. LOAD_NODE_MODULES(X, dirname(Y))
7. THROW "not found"

MAYBE_DETECT_AND_LOAD(X)
1. If X parses as a CommonJS module, load X as a CommonJS module. STOP.
2. Else, if the source code of X can be parsed as ECMAScript module using
<a href="esm.md#resolver-algorithm-specification">DETECT_MODULE_SYNTAX defined in
the ESM resolver</a>,
a. Load X as an ECMAScript module. STOP.
3. THROW the SyntaxError from attempting to parse X as CommonJS in 1. STOP.

LOAD_AS_FILE(X)
1. If X is a file, load X as its file extension format. STOP
2. If X.js is a file,
a. Find the closest package scope SCOPE to X.
b. If no scope was found
1. MAYBE_DETECT_AND_LOAD(X.js)
c. If the SCOPE/package.json contains "type" field,
1. If the "type" field is "module", load X.js as an ECMAScript module. STOP.
2. If the "type" field is "commonjs", load X.js as a CommonJS module. STOP.
d. MAYBE_DETECT_AND_LOAD(X.js)
3. If X.json is a file, load X.json to a JavaScript Object. STOP
4. If X.node is a file, load X.node as binary addon. STOP

LOAD_INDEX(X)
1. If X/index.js is a file
a. Find the closest package scope SCOPE to X.
b. If no scope was found, load X/index.js as a CommonJS module. STOP.
c. If the SCOPE/package.json contains "type" field,
1. If the "type" field is "module", load X/index.js as an ECMAScript module. STOP.
2. Else, load X/index.js as a CommonJS module. STOP.
2. If X/index.json is a file, parse X/index.json to a JavaScript object. STOP
3. If X/index.node is a file, load X/index.node as binary addon. STOP

LOAD_AS_DIRECTORY(X)
1. If X/package.json is a file,
a. Parse X/package.json, and look for "main" field.
b. If "main" is a falsy value, GOTO 2.
c. let M = X + (json main field)
d. LOAD_AS_FILE(M)
e. LOAD_INDEX(M)
f. LOAD_INDEX(X) DEPRECATED
g. THROW "not found"
2. LOAD_INDEX(X)

LOAD_NODE_MODULES(X, START)
1. let DIRS = NODE_MODULES_PATHS(START)
2. for each DIR in DIRS:
a. LOAD_PACKAGE_EXPORTS(X, DIR)
b. LOAD_AS_FILE(DIR/X)
c. LOAD_AS_DIRECTORY(DIR/X)

NODE_MODULES_PATHS(START)
1. let PARTS = path split(START)
2. let I = count of PARTS - 1
3. let DIRS = []
4. while I >= 0,
a. if PARTS[I] = "node_modules", GOTO d.
b. DIR = path join(PARTS[0 .. I] + "node_modules")
c. DIRS = DIR + DIRS
d. let I = I - 1
5. return DIRS + GLOBAL_FOLDERS

LOAD_PACKAGE_IMPORTS(X, DIR)
1. Find the closest package scope SCOPE to DIR.
2. If no scope was found, return.
3. If the SCOPE/package.json "imports" is null or undefined, return.
4. If `--experimental-require-module` is enabled
a. let CONDITIONS = ["node", "require", "module-sync"]
b. Else, let CONDITIONS = ["node", "require"]
5. let MATCH = PACKAGE_IMPORTS_RESOLVE(X, pathToFileURL(SCOPE),
CONDITIONS) <a href="esm.md#resolver-algorithm-specification">defined in the ESM resolver</a>.
6. RESOLVE_ESM_MATCH(MATCH).

LOAD_PACKAGE_EXPORTS(X, DIR)
1. Try to interpret X as a combination of NAME and SUBPATH where the name
may have a @scope/ prefix and the subpath begins with a slash (`/`).
2. If X does not match this pattern or DIR/NAME/package.json is not a file,
return.
3. Parse DIR/NAME/package.json, and look for "exports" field.
4. If "exports" is null or undefined, return.
5. If `--experimental-require-module` is enabled
a. let CONDITIONS = ["node", "require", "module-sync"]
b. Else, let CONDITIONS = ["node", "require"]
6. let MATCH = PACKAGE_EXPORTS_RESOLVE(pathToFileURL(DIR/NAME), "." + SUBPATH,
`package.json` "exports", CONDITIONS) <a href="esm.md#resolver-algorithm-specification">defined in the ESM resolver</a>.
7. RESOLVE_ESM_MATCH(MATCH)

LOAD_PACKAGE_SELF(X, DIR)
1. Find the closest package scope SCOPE to DIR.
2. If no scope was found, return.
3. If the SCOPE/package.json "exports" is null or undefined, return.
4. If the SCOPE/package.json "name" is not the first segment of X, return.
5. let MATCH = PACKAGE_EXPORTS_RESOLVE(pathToFileURL(SCOPE),
"." + X.slice("name".length), `package.json` "exports", ["node", "require"])
<a href="esm.md#resolver-algorithm-specification">defined in the ESM resolver</a>.
6. RESOLVE_ESM_MATCH(MATCH)

RESOLVE_ESM_MATCH(MATCH)
1. let RESOLVED_PATH = fileURLToPath(MATCH)
2. If the file at RESOLVED_PATH exists, load RESOLVED_PATH as its extension
format. STOP
3. THROW "not found" copy
```

### Caching#

Modules are cached after the first time they are loaded. This means (among other things) that every call to require('foo') will get exactly the same object returned, if it would resolve to the same file.

Provided require.cache is not modified, multiple calls to require('foo') will not cause the module code to be executed multiple times. This is an important feature. With it, "partially done" objects can be returned, thus allowing transitive dependencies to be loaded even when they would cause cycles.

To have a module execute code multiple times, export a function, and call that function.

#### Module caching caveats#

Modules are cached based on their resolved filename. Since modules may resolve to a different filename based on the location of the calling module (loading from node_modules folders), it is not a guarantee that require('foo') will always return the exact same object, if it would resolve to different files.

Additionally, on case-insensitive file systems or operating systems, different resolved filenames can point to the same file, but the cache will still treat them as different modules and will reload the file multiple times. For example, require('./foo') and require('./FOO') return two different objects, irrespective of whether or not ./foo and ./FOO are the same file.

### Built-in modules#

History

| Version | Changes |
| v16.0.0, v14.18.0 | Added node: import support to require(...). |

Node.js has several modules compiled into the binary. These modules are described in greater detail elsewhere in this documentation.

The built-in modules are defined within the Node.js source and are located in the lib/ folder.

Built-in modules can be identified using the node: prefix, in which case it bypasses the require cache. For instance, require('node:http') will always return the built in HTTP module, even if there is require.cache entry by that name.

Some built-in modules are always preferentially loaded if their identifier is passed to require(). For instance, require('http') will always return the built-in HTTP module, even if there is a file by that name. The list of built-in modules that can be loaded without using the node: prefix is exposed as module.builtinModules.

#### Built-in modules with mandatory node: prefix#

When being loaded by require(), some built-in modules must be requested with the node: prefix. This requirement exists to prevent newly introduced built-in modules from having a conflict with user land packages that already have taken the name. Currently the built-in modules that requires the node: prefix are:

- node:sea

- node:sqlite

- node:test

- node:test/reporters

### Cycles#

When there are circular require() calls, a module might not have finished executing when it is returned.

Consider this situation:

a.js:

```
console.log('a starting');
exports.done = false;
const b = require('./b.js');
console.log('in a, b.done = %j', b.done);
exports.done = true;
console.log('a done'); copy
```

b.js:

```
console.log('b starting');
exports.done = false;
const a = require('./a.js');
console.log('in b, a.done = %j', a.done);
exports.done = true;
console.log('b done'); copy
```

main.js:

```
console.log('main starting');
const a = require('./a.js');
const b = require('./b.js');
console.log('in main, a.done = %j, b.done = %j', a.done, b.done); copy
```

When main.js loads a.js, then a.js in turn loads b.js. At that point, b.js tries to load a.js. In order to prevent an infinite loop, an unfinished copy of the a.js exports object is returned to the b.js module. b.js then finishes loading, and its exports object is provided to the a.js module.

By the time main.js has loaded both modules, they're both finished. The output of this program would thus be:

```
$ node main.js
main starting
a starting
b starting
in b, a.done = false
b done
in a, b.done = true
a done
in main, a.done = true, b.done = true copy
```

Careful planning is required to allow cyclic module dependencies to work correctly within an application.

### File modules#

If the exact filename is not found, then Node.js will attempt to load the required filename with the added extensions: .js, .json, and finally .node. When loading a file that has a different extension (e.g. .cjs), its full name must be passed to require(), including its file extension (e.g. require('./file.cjs')).

.json files are parsed as JSON text files, .node files are interpreted as compiled addon modules loaded with process.dlopen(). Files using any other extension (or no extension at all) are parsed as JavaScript text files. Refer to the Determining module system section to understand what parse goal will be used.

A required module prefixed with '/' is an absolute path to the file. For example, require('/home/marco/foo.js') will load the file at /home/marco/foo.js.

A required module prefixed with './' is relative to the file calling require(). That is, circle.js must be in the same directory as foo.js for require('./circle') to find it.

Without a leading '/', './', or '../' to indicate a file, the module must either be a core module or is loaded from a node_modules folder.

If the given path does not exist, require() will throw a MODULE_NOT_FOUND error.

### Folders as modules#

Stability: 3 - Legacy: Use subpath exports or subpath imports instead.

There are three ways in which a folder may be passed to require() as an argument.

The first is to create a package.json file in the root of the folder, which specifies a main module. An example package.json file might look like this:

```
{ "name" : "some-library",
"main" : "./lib/some-library.js" } copy
```

If this was in a folder at ./some-library, then require('./some-library') would attempt to load ./some-library/lib/some-library.js.

If there is no package.json file present in the directory, or if the "main" entry is missing or cannot be resolved, then Node.js will attempt to load an index.js or index.node file out of that directory. For example, if there was no package.json file in the previous example, then require('./some-library') would attempt to load:

- ./some-library/index.js

- ./some-library/index.node

If these attempts fail, then Node.js will report the entire module as missing with the default error:

```
Error: Cannot find module 'some-library' copy
```

In all three above cases, an import('./some-library') call would result in a ERR_UNSUPPORTED_DIR_IMPORT error. Using package subpath exports or subpath imports can provide the same containment organization benefits as folders as modules, and work for both require and import.

### Loading from node_modules folders#

If the module identifier passed to require() is not a built-in module, and does not begin with '/', '../', or './', then Node.js starts at the directory of the current module, and adds /node_modules, and attempts to load the module from that location. Node.js will not append node_modules to a path already ending in node_modules.

If it is not found there, then it moves to the parent directory, and so on, until the root of the file system is reached.

For example, if the file at '/home/ry/projects/foo.js' called require('bar.js'), then Node.js would look in the following locations, in this order:

- /home/ry/projects/node_modules/bar.js

- /home/ry/node_modules/bar.js

- /home/node_modules/bar.js

- /node_modules/bar.js

This allows programs to localize their dependencies, so that they do not clash.

It is possible to require specific files or sub modules distributed with a module by including a path suffix after the module name. For instance require('example-module/path/to/file') would resolve path/to/file relative to where example-module is located. The suffixed path follows the same module resolution semantics.

### Loading from the global folders#

If the NODE_PATH environment variable is set to a colon-delimited list of absolute paths, then Node.js will search those paths for modules if they are not found elsewhere.

On Windows, NODE_PATH is delimited by semicolons (;) instead of colons.

NODE_PATH was originally created to support loading modules from varying paths before the current module resolution algorithm was defined.

NODE_PATH is still supported, but is less necessary now that the Node.js ecosystem has settled on a convention for locating dependent modules. Sometimes deployments that rely on NODE_PATH show surprising behavior when people are unaware that NODE_PATH must be set. Sometimes a module's dependencies change, causing a different version (or even a different module) to be loaded as the NODE_PATH is searched.

Additionally, Node.js will search in the following list of GLOBAL_FOLDERS:

- 1: $HOME/.node_modules

- 2: $HOME/.node_libraries

- 3: $PREFIX/lib/node

Where $HOME is the user's home directory, and $PREFIX is the Node.js configured node_prefix.

These are mostly for historic reasons.

It is strongly encouraged to place dependencies in the local node_modules folder. These will be loaded faster, and more reliably.

### The module wrapper#

Before a module's code is executed, Node.js will wrap it with a function wrapper that looks like the following:

```
(function(exports, require, module, __filename, __dirname) {
// Module code actually lives in here
}); copy
```

By doing this, Node.js achieves a few things:

- It keeps top-level variables (defined with var, const, or let) scoped to the module rather than the global object.

- It helps to provide some global-looking variables that are actually specific to the module, such as: The module and exports objects that the implementor can use to export values from the module. The convenience variables __filename and __dirname, containing the module's absolute filename and directory path.

### The module scope#

#### __dirname#

Added in: v0.1.27

- <string>

The directory name of the current module. This is the same as the path.dirname() of the __filename.

Example: running node example.js from /Users/mjr

```
console.log(__dirname);
// Prints: /Users/mjr
console.log(path.dirname(__filename));
// Prints: /Users/mjr copy
```

#### __filename#

Added in: v0.0.1

- <string>

The file name of the current module. This is the current module file's absolute path with symlinks resolved.

For a main program this is not necessarily the same as the file name used in the command line.

See __dirname for the directory name of the current module.

Examples:

Running node example.js from /Users/mjr

```
console.log(__filename);
// Prints: /Users/mjr/example.js
console.log(__dirname);
// Prints: /Users/mjr copy
```

Given two modules: a and b, where b is a dependency of a and there is a directory structure of:

- /Users/mjr/app/a.js

- /Users/mjr/app/node_modules/b/b.js

References to __filename within b.js will return /Users/mjr/app/node_modules/b/b.js while references to __filename within a.js will return /Users/mjr/app/a.js.

#### exports#

Added in: v0.1.12

- <Object>

A reference to the module.exports that is shorter to type. See the section about the exports shortcut for details on when to use exports and when to use module.exports.

#### module#

Added in: v0.1.16

- <module>

A reference to the current module, see the section about the module object. In particular, module.exports is used for defining what a module exports and makes available through require().

#### require(id)#

Added in: v0.1.13

- id <string> module name or path

- Returns: <any> exported module content

Used to import modules, JSON, and local files. Modules can be imported from node_modules. Local modules and JSON files can be imported using a relative path (e.g. ./, ./foo, ./bar/baz, ../foo) that will be resolved against the directory named by __dirname (if defined) or the current working directory. The relative paths of POSIX style are resolved in an OS independent fashion, meaning that the examples above will work on Windows in the same way they would on Unix systems.

```
// Importing a local module with a path relative to the `__dirname` or current
// working directory. (On Windows, this would resolve to .\path\myLocalModule.)
const myLocalModule = require('./path/myLocalModule');

// Importing a JSON file:
const jsonData = require('./path/filename.json');

// Importing a module from node_modules or Node.js built-in module:
const crypto = require('node:crypto'); copy
```

##### require.cache#

Added in: v0.3.0

- <Object>

Modules are cached in this object when they are required. By deleting a key value from this object, the next require will reload the module. This does not apply to native addons, for which reloading will result in an error.

Adding or replacing entries is also possible. This cache is checked before built-in modules and if a name matching a built-in module is added to the cache, only node:-prefixed require calls are going to receive the built-in module. Use with care!

```
const assert = require('node:assert');
const realFs = require('node:fs');

const fakeFs = {};
require.cache.fs = { exports: fakeFs };

assert.strictEqual(require('fs'), fakeFs);
assert.strictEqual(require('node:fs'), realFs); copy
```

##### require.extensions#

Added in: v0.3.0Deprecated since: v0.10.6

Stability: 0 - Deprecated

- <Object>

Instruct require on how to handle certain file extensions.

Process files with the extension .sjs as .js:

```
require.extensions['.sjs'] = require.extensions['.js']; copy
```

Deprecated. In the past, this list has been used to load non-JavaScript modules into Node.js by compiling them on-demand. However, in practice, there are much better ways to do this, such as loading modules via some other Node.js program, or compiling them to JavaScript ahead of time.

Avoid using require.extensions. Use could cause subtle bugs and resolving the extensions gets slower with each registered extension.

##### require.main#

Added in: v0.1.17

- <module> | <undefined>

The Module object representing the entry script loaded when the Node.js process launched, or undefined if the entry point of the program is not a CommonJS module. See "Accessing the main module".

In entry.js script:

```
console.log(require.main); copy
```

```
node entry.js copy
```

```
Module {
id: '.',
path: '/absolute/path/to',
exports: {},
filename: '/absolute/path/to/entry.js',
loaded: false,
children: [],
paths:
[ '/absolute/path/to/node_modules',
'/absolute/path/node_modules',
'/absolute/node_modules',
'/node_modules' ] } copy
```

##### require.resolve(request[, options])#

History

| Version | Changes |
| v8.9.0 | The paths option is now supported. |
| v0.3.0 | Added in: v0.3.0 |

- request <string> The module path to resolve.

- options <Object> paths <string[]> Paths to resolve module location from. If present, these paths are used instead of the default resolution paths, with the exception of GLOBAL_FOLDERS like $HOME/.node_modules, which are always included. Each of these paths is used as a starting point for the module resolution algorithm, meaning that the node_modules hierarchy is checked from this location.

- Returns: <string>

Use the internal require() machinery to look up the location of a module, but rather than loading the module, just return the resolved filename.

If the module can not be found, a MODULE_NOT_FOUND error is thrown.

###### require.resolve.paths(request)#

Added in: v8.9.0

- request <string> The module path whose lookup paths are being retrieved.

- Returns: <string[]> | <null>

Returns an array containing the paths searched during resolution of request or null if the request string references a core module, for example http or fs.

### The module object#

Added in: v0.1.16

- <Object>

In each module, the module free variable is a reference to the object representing the current module. For convenience, module.exports is also accessible via the exports module-global. module is not actually a global but rather local to each module.

#### module.children#

Added in: v0.1.16

- <module[]>

The module objects required for the first time by this one.

#### module.exports#

Added in: v0.1.16

- <Object>

The module.exports object is created by the Module system. Sometimes this is not acceptable; many want their module to be an instance of some class. To do this, assign the desired export object to module.exports. Assigning the desired object to exports will simply rebind the local exports variable, which is probably not what is desired.

For example, suppose we were making a module called a.js:

```
const EventEmitter = require('node:events');

module.exports = new EventEmitter();

// Do some work, and after some time emit
// the 'ready' event from the module itself.
setTimeout(() => {
module.exports.emit('ready');
}, 1000); copy
```

Then in another file we could do:

```
const a = require('./a');
a.on('ready', () => {
console.log('module "a" is ready');
}); copy
```

Assignment to module.exports must be done immediately. It cannot be done in any callbacks. This does not work:

x.js:

```
setTimeout(() => {
module.exports = { a: 'hello' };
}, 0); copy
```

y.js:

```
const x = require('./x');
console.log(x.a); copy
```

##### exports shortcut#

Added in: v0.1.16

The exports variable is available within a module's file-level scope, and is assigned the value of module.exports before the module is evaluated.

It allows a shortcut, so that module.exports.f = ... can be written more succinctly as exports.f = .... However, be aware that like any variable, if a new value is assigned to exports, it is no longer bound to module.exports:

```
module.exports.hello = true; // Exported from require of module
exports = { hello: false }; // Not exported, only available in the module copy
```

When the module.exports property is being completely replaced by a new object, it is common to also reassign exports:

```
module.exports = exports = function Constructor() {
// ... etc.
}; copy
```

To illustrate the behavior, imagine this hypothetical implementation of require(), which is quite similar to what is actually done by require():

```
function require(/* ... */) {
const module = { exports: {} };
((module, exports) => {
// Module code here. In this example, define a function.
function someFunc() {}
exports = someFunc;
// At this point, exports is no longer a shortcut to module.exports, and
// this module will still export an empty default object.
module.exports = someFunc;
// At this point, the module will now export someFunc, instead of the
// default object.
})(module, module.exports);
return module.exports;
} copy
```

#### module.filename#

Added in: v0.1.16

- <string>

The fully resolved filename of the module.

#### module.id#

Added in: v0.1.16

- <string>

The identifier for the module. Typically this is the fully resolved filename.

#### module.isPreloading#

Added in: v15.4.0, v14.17.0

- Type: <boolean> true if the module is running during the Node.js preload phase.

#### module.loaded#

Added in: v0.1.16

- <boolean>

Whether or not the module is done loading, or is in the process of loading.

#### module.parent#

Added in: v0.1.16Deprecated since: v14.6.0, v12.19.0

Stability: 0 - Deprecated: Please use require.main and
module.children instead.

- <module> | <null> | <undefined>

The module that first required this one, or null if the current module is the entry point of the current process, or undefined if the module was loaded by something that is not a CommonJS module (E.G.: REPL or import).

#### module.path#

Added in: v11.14.0

- <string>

The directory name of the module. This is usually the same as the path.dirname() of the module.id.

#### module.paths#

Added in: v0.4.0

- <string[]>

The search paths for the module.

#### module.require(id)#

Added in: v0.5.1

- id <string>

- Returns: <any> exported module content

The module.require() method provides a way to load a module as if require() was called from the original module.

In order to do this, it is necessary to get a reference to the module object. Since require() returns the module.exports, and the module is typically only available within a specific module's code, it must be explicitly exported in order to be used.

### The Module object#

This section was moved to Modules: module core module.

- module.builtinModules

- module.createRequire(filename)

- module.syncBuiltinESMExports()

### Source map v3 support#

This section was moved to Modules: module core module.

- module.findSourceMap(path)

- Class: module.SourceMap new SourceMap(payload) sourceMap.payload sourceMap.findEntry(lineNumber, columnNumber)
