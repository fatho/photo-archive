export class HashRouter {
    private routes: Array<Route>;

    constructor() {
        this.routes = new Array();

        window.addEventListener('hashchange', (ev: HashChangeEvent) => {
            this.route()
        });
    }

    /// Return whether there is an explicit route set.
    hasRoute(): boolean {
        return window.location.hash.length > 0;
    }

    navigate(path: string[], replace: boolean = false) {
        if(replace) {
            location.replace(`#${path.join('/')}`);
        } else {
            window.location.hash = path.join('/');
        }
    }

    /// Apply the routing logic to the currently set path.
    route() {
        let hash = window.location.hash;
        if(hash.startsWith('#')) {
            hash = hash.substring(1);
        }

        let path: string[] = hash.split('/')

        console.log("Routing: %s", path);

        for(let i = 0; i < this.routes.length; i++) {
            let args = this.routes[i].match(path);
            if (args != null) {
                this.routes[i].call(args);
                return;
            }
        }

        throw new Error('No route');
    }

    addRoute(pattern: Array<string | RouteParam>, handler: (...args: any[]) => void) {
        this.routes.push(new Route(pattern, handler));
    }
}

export class Route {
    private handler: (...args: any[]) => void;
    private pattern: Array<string | RouteParam>;

    constructor(pattern: Array<string | RouteParam>, handler: (...args: any[]) => void) {
        this.pattern = pattern;
        this.handler = handler;
    }

    match(path: string[]): Array<any> | null {
        if (path.length != this.pattern.length) {
            return null;
        }

        let parsedArgs = new Array();

        for(let i = 0; i < path.length; i++) {
            let patternComponent = this.pattern[i];
            let pathComponent = path[i];

            if (typeof patternComponent == 'string') {
                if (patternComponent != pathComponent) {
                    return null;
                }
            } else if (patternComponent instanceof RouteParam) {
                let arg = patternComponent.parse(pathComponent);
                if (arg == null) {
                    return null;
                } else {
                    parsedArgs.push(arg);
                }
            }
        }

        return parsedArgs;
    }

    call(args: any[]) {
        this.handler(...args);
    }
}


export class RouteParam {
    private validator: RegExp;
    private parser: (value: string) => any;

    constructor(validator: RegExp, parser: (value: string) => any) {
        this.validator = validator;
        this.parser = parser;
    }

    parse(value: string): any | null {
        if (! this.validator.test(value)) {
            return null;
        }
        return this.parser(value);
    }

    static int(): RouteParam {
        return new RouteParam(/^-?[0-9]+$/, (value) => parseInt(value, 10))
    }
}
