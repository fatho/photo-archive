import { Page } from "./pages/Page";
import { GalleryPage } from "./pages/Gallery";
import { HashRouter, RouteParam } from "./routing/HashRouter";
import { AppState } from "./State";

class App {
    private current_page: Page | null;
    private root_id: string;

    constructor(root_id: string) {
        this.current_page = null;
        this.root_id = root_id;
    }

    goto(page: Page) {
        let root = document.getElementById(this.root_id) as HTMLElement;
        if (page != this.current_page) {
            if (this.current_page != null) {
                this.current_page.detach(root);
            }
            if(page != null) {
                page.attach(root);
            }
            this.current_page = page;
        }
    }
}

let app = new App('root');
let state = new AppState();
state.requestPhotos();

class Pages {
    static gallery: GalleryPage = new GalleryPage(state);
}

let router = new HashRouter();

router.addRoute(['gallery'], () => {
    app.goto(Pages.gallery);
});

router.addRoute(['slideshow', RouteParam.int()], (id: number) => {
    console.log("slideshow %d", id);
    app.goto(Pages.gallery);
});

window.onload = () => {
    if (router.hasRoute()) {
        router.route();
    } else {
        router.navigate(["gallery"]);
    }
}
