import { VirtualGrid } from "./components/VirtualGrid"
import { Page } from "./pages/Page";
import { GalleryPage } from "./pages/Gallery";
import { Lru } from "./util/Cache";

class App {
    current_page: Page;
    root_id: string;

    constructor(root_id: string) {
        this.current_page = new GalleryPage();
        this.current_page.enter();
        this.root_id = root_id;
    }

    render() {
        let root = document.getElementById(this.root_id) as HTMLElement;
        this.current_page.render(root);
    }
}

let app = new App('root');

window.onload = () => {
    app.render();
}
