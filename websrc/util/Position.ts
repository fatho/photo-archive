export class Position {
    element: HTMLElement;

    constructor(element: HTMLElement) {
        this.element = element;
    }

    fill() {
        this.element.style.left = "0px";
        this.element.style.right = "0px";
        this.element.style.top = "0px";
        this.element.style.bottom = "0px";
    }

    top(top: string): Position {
        this.element.style.top = top;
        return this;
    }

    left(left: string): Position {
        this.element.style.left = left;
        return this;
    }

    right(right: string): Position {
        this.element.style.right = right;
        return this;
    }

    bottom(bottom: string): Position {
        this.element.style.bottom = bottom;
        return this;
    }

    width(width: string): Position {
        this.element.style.width = width;
        return this;
    }

    height(height: string): Position {
        this.element.style.height = height;
        return this;
    }

    static absolute(element: HTMLElement): Position {
        element.style.position = "absolute";
        return new Position(element);
    }
}