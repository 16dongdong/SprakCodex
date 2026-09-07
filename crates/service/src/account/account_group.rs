use codexmanager_core::storage::Account;

pub(crate) fn normalize_account_group_filter(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub(crate) fn account_matches_group_filter(
    account: &Account,
    account_group_filter: Option<&str>,
) -> bool {
    let Some(group_filter) = account_group_filter
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return true;
    };

    account
        .group_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        == Some(group_filter)
}

#[cfg(test)]
mod tests {
    use super::{account_matches_group_filter, normalize_account_group_filter};
    use codexmanager_core::storage::Account;

    fn account(group_name: Option<&str>) -> Account {
        Account {
            id: "acc-1".to_string(),
            label: "Account".to_string(),
            issuer: "chatgpt".to_string(),
            chatgpt_account_id: None,
            workspace_id: None,
            group_name: group_name.map(str::to_string),
            sort: 0,
            status: "active".to_string(),
            created_at: 1,
            updated_at: 1,
        }
    }

    #[test]
    fn group_filter_trims_and_empty_means_all_accounts() {
        assert_eq!(
            normalize_account_group_filter(Some("  team-a  ".to_string())).as_deref(),
            Some("team-a")
        );
        assert_eq!(
            normalize_account_group_filter(Some("   ".to_string())),
            None
        );
        assert!(account_matches_group_filter(&account(None), None));
    }

    #[test]
    fn group_filter_is_trimmed_exact_and_case_sensitive() {
        let candidate = account(Some("  Team-A  "));
        assert!(account_matches_group_filter(&candidate, Some("Team-A")));
        assert!(!account_matches_group_filter(&candidate, Some("team-a")));
        assert!(!account_matches_group_filter(
            &account(None),
            Some("Team-A")
        ));
    }
}
