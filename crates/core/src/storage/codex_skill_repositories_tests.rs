use super::*;

fn storage() -> Storage {
    let storage = Storage::open_in_memory().expect("open in-memory storage");
    storage.init().expect("initialize storage");
    storage
}

fn custom_repository(id: &str) -> CodexSkillRepositoryUpsert {
    CodexSkillRepositoryUpsert {
        id: id.to_string(),
        owner: "example".to_string(),
        repository: "skills".to_string(),
        ref_name: String::new(),
        enabled: true,
        is_builtin: false,
        created_at: 10,
        updated_at: 10,
    }
}

fn skill(
    repository_id: &str,
    skill_id: &str,
    name: &str,
    path: &str,
    revision: &str,
) -> CodexSkillRepositorySkillRecord {
    CodexSkillRepositorySkillRecord {
        repository_id: repository_id.to_string(),
        skill_id: skill_id.to_string(),
        name: name.to_string(),
        description: format!("Use {name} when testing repository snapshots."),
        path: path.to_string(),
        source_url: format!("https://github.com/example/skills/tree/{revision}/{path}"),
        revision: Some(revision.to_string()),
    }
}

#[test]
fn migration_seeds_builtin_repositories_idempotently() {
    let storage = storage();
    let repositories = storage
        .list_codex_skill_repositories()
        .expect("list seeded repositories");
    assert_eq!(repositories.len(), 4);
    assert!(repositories
        .iter()
        .all(|repository| repository.enabled && repository.is_builtin));

    let expected = [
        ("anthropics", "skills", "main"),
        ("ComposioHQ", "awesome-claude-skills", "master"),
        ("cexll", "myclaude", "master"),
        ("JimLiu", "baoyu-skills", "main"),
    ];
    for (owner, repository, ref_name) in expected {
        assert!(repositories.iter().any(|item| {
            item.owner == owner
                && item.repository == repository
                && item.ref_name == ref_name
                && item.last_scanned_at.is_none()
                && item.last_error.is_none()
        }));
    }

    assert!(storage
        .set_codex_skill_repository_enabled("builtin-anthropics-skills", false)
        .expect("disable builtin"));
    storage
        .conn
        .execute_batch(include_str!(
            "../../migrations/124_codex_skill_repositories.sql"
        ))
        .expect("reapply idempotent migration");

    let repositories = storage
        .list_codex_skill_repositories()
        .expect("list repositories after migration replay");
    assert_eq!(repositories.len(), 4);
    assert!(
        !storage
            .get_codex_skill_repository("builtin-anthropics-skills")
            .expect("read builtin")
            .expect("builtin exists")
            .enabled
    );
}

#[test]
fn repository_upsert_updates_custom_rows_but_preserves_builtin_identity() {
    let storage = storage();
    let mut input = custom_repository("custom-example-skills");
    let inserted = storage
        .upsert_codex_skill_repository(&input)
        .expect("insert repository");
    assert_eq!(inserted.owner, "example");
    assert_eq!(inserted.repository, "skills");
    assert_eq!(inserted.created_at, 10);
    assert_eq!(inserted.updated_at, 10);

    input.owner = "updated-owner".to_string();
    input.repository = "updated-repository".to_string();
    input.ref_name = "main".to_string();
    input.enabled = false;
    input.created_at = 99;
    input.updated_at = 20;
    let updated = storage
        .upsert_codex_skill_repository(&input)
        .expect("update repository");
    assert_eq!(updated.owner, "updated-owner");
    assert_eq!(updated.repository, "updated-repository");
    assert_eq!(updated.ref_name, "main");
    assert!(!updated.enabled);
    assert_eq!(updated.created_at, 10);
    assert_eq!(updated.updated_at, 20);

    let builtin = storage
        .get_codex_skill_repository("builtin-anthropics-skills")
        .expect("read builtin")
        .expect("builtin exists");
    let protected = storage
        .upsert_codex_skill_repository(&CodexSkillRepositoryUpsert {
            id: builtin.id.clone(),
            owner: "attacker".to_string(),
            repository: "replacement".to_string(),
            ref_name: "other".to_string(),
            enabled: false,
            is_builtin: false,
            created_at: 1,
            updated_at: 30,
        })
        .expect("update builtin enabled state");
    assert_eq!(protected.owner, "anthropics");
    assert_eq!(protected.repository, "skills");
    assert_eq!(protected.ref_name, "main");
    assert!(protected.is_builtin);
    assert!(!protected.enabled);

    assert!(!storage
        .delete_codex_skill_repository(&protected.id)
        .expect("refuse builtin deletion"));
    assert!(storage
        .delete_codex_skill_repository(&updated.id)
        .expect("delete custom repository"));
    assert!(storage
        .get_codex_skill_repository(&updated.id)
        .expect("read deleted repository")
        .is_none());
}

#[test]
fn snapshot_replacement_is_atomic_and_scan_errors_preserve_last_good_data() {
    let storage = storage();
    let repository_id = "custom-snapshot";
    storage
        .upsert_codex_skill_repository(&custom_repository(repository_id))
        .expect("insert repository");
    assert!(storage
        .record_codex_skill_repository_error(repository_id, "initial failure")
        .expect("record initial error"));

    let first = vec![
        skill(repository_id, "skill-a", "alpha", "skills/alpha", "rev-a"),
        skill(repository_id, "skill-b", "beta", "nested/beta", "rev-a"),
    ];
    storage
        .replace_codex_skill_repository_snapshot(repository_id, &first, 100)
        .expect("replace first snapshot");

    let repository = storage
        .get_codex_skill_repository(repository_id)
        .expect("read repository")
        .expect("repository exists");
    assert_eq!(repository.revision.as_deref(), Some("rev-a"));
    assert_eq!(repository.last_scanned_at, Some(100));
    assert!(repository.last_error.is_none());
    assert_eq!(
        storage
            .get_codex_skill_repository_skill(repository_id, "skill-b")
            .expect("read skill")
            .expect("skill exists")
            .path,
        "nested/beta"
    );

    let second = vec![skill(
        repository_id,
        "skill-c",
        "gamma",
        "skills/gamma",
        "rev-b",
    )];
    storage
        .replace_codex_skill_repository_snapshot(repository_id, &second, 200)
        .expect("replace second snapshot");
    assert_eq!(
        storage
            .list_codex_skill_repository_skills(repository_id)
            .expect("list second snapshot"),
        second
    );

    let invalid = vec![
        skill(repository_id, "duplicate-a", "first", "same/path", "rev-c"),
        skill(repository_id, "duplicate-b", "second", "SAME/PATH", "rev-c"),
    ];
    assert!(storage
        .replace_codex_skill_repository_snapshot(repository_id, &invalid, 300)
        .is_err());
    let mixed_revision = vec![
        skill(repository_id, "mixed-a", "mixed-a", "mixed/a", "rev-c"),
        skill(repository_id, "mixed-b", "mixed-b", "mixed/b", "rev-d"),
    ];
    assert!(storage
        .replace_codex_skill_repository_snapshot(repository_id, &mixed_revision, 300)
        .is_err());
    assert!(storage
        .replace_codex_skill_repository_snapshot(repository_id, &[], 300)
        .is_err());

    let repository = storage
        .get_codex_skill_repository(repository_id)
        .expect("read repository after rollback")
        .expect("repository exists after rollback");
    assert_eq!(repository.revision.as_deref(), Some("rev-b"));
    assert_eq!(repository.last_scanned_at, Some(200));
    assert_eq!(
        storage
            .list_codex_skill_repository_skills(repository_id)
            .expect("list snapshot after rollback"),
        second
    );

    assert!(storage
        .record_codex_skill_repository_error(repository_id, "refresh failed")
        .expect("record refresh error"));
    let catalog = storage
        .codex_skill_repository_catalog_snapshot()
        .expect("read catalog snapshot");
    let repository = catalog
        .repositories
        .iter()
        .find(|item| item.id == repository_id)
        .expect("catalog repository");
    assert_eq!(repository.last_error.as_deref(), Some("refresh failed"));
    assert_eq!(repository.last_scanned_at, Some(200));
    assert_eq!(catalog.skills, second);

    assert!(storage
        .delete_codex_skill_repository(repository_id)
        .expect("delete repository"));
    assert!(storage
        .list_codex_skill_repository_skills(repository_id)
        .expect("list cascaded skills")
        .is_empty());
}

#[test]
fn snapshot_rejects_unknown_or_mismatched_repositories() {
    let storage = storage();
    let repository_id = "custom-mismatch";
    storage
        .upsert_codex_skill_repository(&custom_repository(repository_id))
        .expect("insert repository");

    let mismatched = vec![skill(
        "different-repository",
        "skill-a",
        "alpha",
        "skills/alpha",
        "rev-a",
    )];
    assert!(storage
        .replace_codex_skill_repository_snapshot(repository_id, &mismatched, 100)
        .is_err());
    assert!(storage
        .list_codex_skill_repository_skills(repository_id)
        .expect("list after mismatch")
        .is_empty());

    assert!(storage
        .replace_codex_skill_repository_snapshot(
            "missing-repository",
            &Vec::<CodexSkillRepositorySkillRecord>::new(),
            100,
        )
        .is_err());
}
