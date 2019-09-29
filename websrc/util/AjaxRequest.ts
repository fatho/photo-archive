export class Request {
    static get(path: string): RequestBuilder {
        return new RequestBuilder('GET', path);
    }
}

export class RequestBuilder {
    method: string;
    path: string;

    onfailure: ((r: Response) => void) | null = null;
    onsuccess: ((r: Response) => void) | null = null;

    constructor(method: string, path: string) {
        this.method = method;
        this.path = path;
    }

    onFailure(callback: (r: Response) => void): RequestBuilder {
        this.onfailure = callback;
        return this;
    }


    onSuccess(callback: (r: Response) => void): RequestBuilder {
        this.onsuccess = callback;
        return this;
    }

    send() {
        let req = new XMLHttpRequest();
        req.open(this.method, this.path);
        let that = this;
        req.onreadystatechange = function(this: XMLHttpRequest, _event: Event) {
            if (this.readyState == XMLHttpRequest.DONE) {
                if (this.status >= 200 && this.status < 300) {
                    if(that.onsuccess) {
                        that.onsuccess(new Response(this));
                    }
                } else {
                    if(that.onfailure) {
                        that.onfailure(new Response(this));
                    }
                }
            }
        };
        req.send();
    }
}

export class Response {
    req: XMLHttpRequest;

    constructor(req: XMLHttpRequest) {
        this.req = req;
    }

    json(): any {
        return JSON.parse(this.req.responseText);
    }

    text(): string {
        return this.req.responseText;
    }
}