import { Page } from "./Page";
import { Request } from "../util/AjaxRequest";
import { VirtualGrid } from "../components/VirtualGrid";
import { Position } from "../util/Position";

export class GalleryPage implements Page {
    imageGrid: VirtualGrid;
    photos: Photo[];

    constructor() {
        this.imageGrid = VirtualGrid.createElement();
        Position.absolute(this.imageGrid).fill();
        this.imageGrid.cellWidth = 320;
        this.imageGrid.cellHeight = 240;
        this.photos = new Array();
    }

    enter(): void {
        Request.get('/photos')
            .onSuccess(r => this.receivePhotos(r.json()))
            .onFailure(r => this.failedPhotos(r.text()))
            .send();
    }

    leave(): void {}

    failedPhotos(message: string) {
        console.log(message);
    }

    receivePhotos(photos: Photo[]) {
        console.log(photos.length);
        this.photos = photos;
        this.imageGrid.virtualElementCount = this.photos.length;
        // TODO: referesh image grid
    }

    render(root: HTMLElement): void {
        root.appendChild(this.imageGrid);
    }
}

type Photo = {
    id: number,
    relative_path: string,
    created: string,
};