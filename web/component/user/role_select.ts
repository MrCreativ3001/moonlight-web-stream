import { UndetailedRole } from "../../api_bindings.js";
import { getCurrentLanguage, getTranslations } from "../../i18n.js";
import { SelectComponent } from "../input.js";
import { formatRoleName } from "../roles/index.js";

export function createSelectRoleInput(roles: Array<UndetailedRole>, preselectedId?: number): SelectComponent {
    const i = getTranslations(getCurrentLanguage()).admin
    let defaultRole = null
    // Try to find the preselected option
    for (const role of roles) {
        if (role.id == preselectedId) {
            defaultRole = role
            break
        }
    }

    // The default should be the user role, if no preselected was found
    if (!defaultRole) {
        for (const role of roles) {
            if (role.name == "User") {
                defaultRole = role
                break
            }
        }
    }

    // Still no hit for the role -> just use the last role (the first is likely the admin)
    if (!defaultRole) {
        defaultRole = roles[roles.length - 1]
    }

    return new SelectComponent(
        "role",
        roles.map(role => {
            return { value: `${role.id}`, name: formatRoleName(role) }
        }),
        {
            displayName: i.role,
            preSelectedOption: defaultRole && `${defaultRole.id}`
        }
    )
}
