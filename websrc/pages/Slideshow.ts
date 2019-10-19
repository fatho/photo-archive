import { Page } from "./Page";
import { Position } from "../util/Position";
import { AppState, StateChangedListener, Photo } from "../State";
import { HashRouter } from "../routing/HashRouter";

export class SldeshowPage implements Page, StateChangedListener {
    private _imageContainer: HTMLElement;
    private _currentIndex: number;
    private _keyListener: (evt: KeyboardEvent) => void;
    router: HashRouter;
    state: AppState;

    constructor(state: AppState, router: HashRouter) {
        this.router = router;
        this.state = state;
        this._currentIndex = -1;

        let that = this;
        this._keyListener = (evt: KeyboardEvent) => {
            switch(evt.key) {
                case "ArrowRight":
                    that.nextPhoto();
                    evt.preventDefault();
                    break;
                case "ArrowLeft":
                    that.previousPhoto();
                    evt.preventDefault();
                    break;
                case "Escape":
                    window.history.back();
                    break;
            }
        };

        // setup flexbox layout
        this._imageContainer = document.createElement('div');
        this._imageContainer.style.backgroundSize = 'fill';
        this._imageContainer.style.backgroundPosition = 'center';
        this._imageContainer.style.backgroundRepeat = 'no-repeat';
        this._imageContainer.style.backgroundSize = 'contain';
        this._imageContainer.style.backgroundColor = 'black';
        // make the container fill all of its parent
        Position.absolute(this._imageContainer).fill();
    }

    private displayCurrentImage() {
        if(this._currentIndex >= 0 && this._currentIndex < this.state.photos.length) {
            let photo = this.state.photos[this._currentIndex];
            this._imageContainer.style.backgroundImage = `url("/photos/${photo.id}/original")`
        } else {
            this._imageContainer.style.backgroundImage = '';
        }
    }

    public get currentIndex() : number {
        return this._currentIndex;
    }

    public set currentIndex(v : number) {
        if(v != this._currentIndex) {
            this._currentIndex = v;
            this.displayCurrentImage();
        }
    }

    public nextPhoto(): void {
        if(this.currentIndex >= 0) {
            this.gotoPhoto((this.currentIndex + 1) % this.state.photos.length);
        }
    }

    public previousPhoto(): void {
        if(this.currentIndex >= 0) {
            this.gotoPhoto((this.currentIndex + this.state.photos.length - 1) % this.state.photos.length);
        }
    }

    public gotoPhoto(index: number): void {
        this.router.navigate(['slideshow', index.toString()], true);
    }

    photosChanged(_state: AppState, oldPhotos: Photo[], newPhotos: Photo[]): void {
        // Try to find photo:
        if(this._currentIndex >= 0 && this._currentIndex < oldPhotos.length) {
            let currentPhoto = oldPhotos[this._currentIndex];
            let newIndex = newPhotos.findIndex((photo) => photo.id == currentPhoto.id);
            this._currentIndex = newIndex;
        }
        this.displayCurrentImage();
    }

    attach(root: HTMLElement): void {
        root.appendChild(this._imageContainer);
        window.addEventListener("keydown", this._keyListener);
        this.state.addPhotosChangedListener(this)
        this.displayCurrentImage();
    }

    detach(root: HTMLElement): void {
        this.state.removePhotosChangedListener(this)
        window.removeEventListener("keydown", this._keyListener);
        root.removeChild(this._imageContainer);
    }
}
