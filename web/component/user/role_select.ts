import { UserRole } from "../../api_bindings.js";
import { SelectComponent } from "../input.js";

export function createSelectRoleInput(preselected?: UserRole): SelectComponent {
    return new SelectComponent("role", [
        { value: "User", name: "User" },
        { value: "Admin", name: "Admin" },
    ], {
        displayName: "Role",
        preSelectedOption: preselected
    })
}