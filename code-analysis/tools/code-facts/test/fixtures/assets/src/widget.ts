import "./widget.css";
import "bundler!./widget.css";
import { help } from "./help.js";

export const widget = (): string => help();
