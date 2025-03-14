use super::{admin::IdpAdmin, ext::IdpConfig};
use crate::Role;
use std::env;

// Helper function to create a realistic test config based on Terraform setup
fn create_keycloak_config() -> IdpConfig {
    IdpConfig {
        provider_type: "keycloak".to_string(),
        base_url: "http://keycloak:8080".to_string(),
        realm: Some("app-realm".to_string()),
        client_id: "rust-app".to_string(),
        client_secret: Some(
            env::var("KEYCLOAK_CLIENT_SECRET").unwrap_or_else(|_| "test-client-secret".to_string()),
        ),
        admin_username: Some(env::var("KEYCLOAK_ADMIN").unwrap_or_else(|_| "root".to_string())),
        admin_password: Some(
            env::var("KEYCLOAK_ADMIN_PASSWORD").unwrap_or_else(|_| "root".to_string()),
        ),
        service_account_key_path: None,
        domain: None,
    }
}

// Test that verifies IdpAdmin can be created with the Keycloak configuration
#[tracing_test::traced_test]
#[tokio::test]
async fn test_create_idp_admin() {
    let result = IdpAdmin::new(create_keycloak_config()).await;
    assert!(
        result.is_ok(),
        "Failed to create IdpAdmin: {:?}",
        result.err()
    );
}

// Test Team1 member retrieval
#[tracing_test::traced_test]
#[tokio::test]
async fn test_team1_member_retrieval() {
    let admin = IdpAdmin::new(create_keycloak_config())
        .await
        .expect("Failed to create IdpAdmin");

    // Test retrieving captain1 from Team1
    let captain1_result = admin.find_users_by_username("captain1").await;
    assert!(
        captain1_result.is_ok(),
        "Failed to retrieve user captain1: {:?}",
        captain1_result.err()
    );

    let admins = captain1_result.unwrap();
    assert_eq!(admins.len(), 1);
    let captain1 = &admins[0];
    assert_eq!(captain1.username, "captain1");
    assert_eq!(captain1.email, "captain1@test.com".to_string());
    assert_eq!(captain1.first_name, Some("Charles".to_string()));

    // Test retrieving by UUID
    let captain1_by_id = admin.get_user(captain1.id).await;
    assert!(
        captain1_by_id.is_ok(),
        "Failed to retrieve user by ID: {:?}",
        captain1_by_id.err()
    );

    // Test team member retrieval
    let member1_result = admin.find_users_by_username("member1").await;
    assert!(
        member1_result.is_ok(),
        "Failed to retrieve team member: {:?}",
        member1_result.err()
    );

    let members = member1_result.unwrap();
    assert_eq!(members.len(), 1);
    assert_eq!(members[0].username, "member1");
}

// Test teams retrieval
#[tracing_test::traced_test]
#[tokio::test]
async fn test_get_teams() {
    let admin = IdpAdmin::new(create_keycloak_config())
        .await
        .expect("Failed to create IdpAdmin");
    let teams = admin.get_teams().await;
    assert!(teams.is_ok(), "Failed to retrieve teams");
    let teams = teams.unwrap();
    assert_eq!(teams.len(), 3);
}

// Test role assignment for different user types
#[tracing_test::traced_test]
#[tokio::test]
async fn test_get_institutions() {
    let admin = IdpAdmin::new(create_keycloak_config())
        .await
        .expect("Failed to create IdpAdmin");
    let institutions = admin.get_institutions().await;
    assert!(institutions.is_ok(), "Failed to retrieve institutions");
    let institutions = institutions.unwrap();
    assert_eq!(institutions.len(), 2);
}

// Test role assignment for different user types
#[tracing_test::traced_test]
#[tokio::test]
async fn test_role_assignments() {
    let admin = IdpAdmin::new(create_keycloak_config())
        .await
        .expect("Failed to create IdpAdmin");

    // First find admin user
    let admin_result = admin.find_users_by_username("admin").await;
    assert!(
        admin_result.is_ok(),
        "Failed to retrieve admin user: {:?}",
        admin_result.err()
    );

    let admins = admin_result.unwrap();
    assert!(!admins.is_empty(), "admin user not found");
    let admin_user = &admins[0];

    // Test admin user roles
    let admin_roles_result = admin.get_user_roles(admin_user.id).await;
    assert!(
        admin_roles_result.is_ok(),
        "Failed to retrieve admin's roles: {:?}",
        admin_roles_result.err()
    );

    let admin_roles = admin_roles_result.unwrap();
    let has_admin_role = admin_roles.iter().any(|role| role.name == "admin");
    assert!(has_admin_role, "admin user should have admin role");

    // Test team captain roles
    let captain1_result = admin.find_users_by_username("captain1").await.unwrap();
    let captain1 = &captain1_result[0];
    let captain1_roles = admin.get_user_roles(captain1.id).await.unwrap();
    let has_captain_role = captain1_roles.iter().any(|role| role.name == "captain");
    assert!(has_captain_role, "Team admin should have CAPTAIN role");

    // Test advisor roles
    let advisor_result = admin.find_users_by_username("advisor1").await;
    assert!(advisor_result.is_ok(), "Failed to find advisor user");

    let advisors = advisor_result.unwrap();
    assert!(!advisors.is_empty(), "Advisor not found");
    let advisor = &advisors[0];

    let advisor_roles = admin.get_user_roles(advisor.id).await.unwrap();
    let has_advisor_role = advisor_roles.iter().any(|role| role.name == "advisor");
    assert!(
        has_advisor_role,
        "University advisor should have ADVISOR role"
    );
}

// Test domain model conversion for different user types
#[tracing_test::traced_test]
#[tokio::test]
async fn test_domain_model_conversion() {
    let admin = IdpAdmin::new(create_keycloak_config())
        .await
        .expect("Failed to create IdpAdmin");

    // Test conversion for team captain
    let captain1_result = admin.find_users_by_username("captain1").await.unwrap();
    let domain_user_result = admin.to_domain_user(&captain1_result[0]).await;
    assert!(
        domain_user_result.is_ok(),
        "Failed to convert team captain to domain model: {:?}",
        domain_user_result.err()
    );

    let captain_domain = domain_user_result.unwrap();
    assert_eq!(captain_domain.username, "captain1");

    // Verify team info
    assert!(
        captain_domain.team.is_some(),
        "Captain should have team info"
    );
    if let Some(team) = captain_domain.team {
        assert_eq!(team.name, "Team1");
    }

    // Verify role (CAPTAIN should map to ADMIN in domain model)
    assert!(
        matches!(captain_domain.role, Role::Captain),
        "Captain should map to ADMIN role, got {:?}",
        captain_domain.role
    );

    // Test conversion for advisor
    let advisor_result = admin.find_users_by_username("advisor1").await.unwrap();
    let advisor_domain = admin.to_domain_user(&advisor_result[0]).await.unwrap();

    // Verify team info for advisor
    assert!(
        advisor_domain.institution.is_some(),
        "Advisor should have institution info"
    );
    if let Some(team) = advisor_domain.institution {
        assert_eq!(team.name, "School1");
    }

    // Verify role mapping for advisor
    assert!(
        matches!(advisor_domain.role, Role::Advisor),
        "Advisor should map to ACADEMIC_ADVISOR role, got {:?}",
        advisor_domain.role
    );

    // Test conversion for global viewer
    let viewer_result = admin.find_users_by_username("viewer_global").await.unwrap();
    let viewer_domain = admin.to_domain_user(&viewer_result[0]).await.unwrap();

    // Viewers should have no team
    assert!(
        viewer_domain.team.is_none(),
        "Global viewer should not have team assignment"
    );

    // Verify role mapping for viewer
    assert!(
        matches!(viewer_domain.role, Role::Spectator),
        "Viewer should map to SPECTATOR role, got {:?}",
        viewer_domain.role
    );
}

// Test group membership and attributes
#[tracing_test::traced_test]
#[tokio::test]
async fn test_group_membership_and_attributes() {
    let admin = IdpAdmin::new(create_keycloak_config())
        .await
        .expect("Failed to create IdpAdmin");

    // Get Stanford advisor
    let advisor_result = admin.find_users_by_username("advisor1").await.unwrap();
    let advisor = &advisor_result[0];

    // Get advisor's groups
    let advisor_groups = admin.get_user_groups(advisor.id).await;
    assert!(
        advisor_groups.is_ok(),
        "Failed to get advisor's groups: {:?}",
        advisor_groups.err()
    );

    let groups = advisor_groups.unwrap();
    assert!(
        !groups.is_empty(),
        "Advisor should belong to at least one group"
    );

    // Find School1 group
    let school1_group = groups.iter().find(|g| g.name == "School1");
    assert!(
        school1_group.is_some(),
        "School1 advisor should belong to School1 group"
    );

    if let Some(group) = school1_group {
        // Check for region code via parent group
        if let Some(parent_id) = group.parent_id {
            let parent_group = admin.get_group(parent_id).await;
            assert!(parent_group.is_ok(), "Failed to get parent group");

            let universities_group = parent_group.unwrap();
            assert_eq!(
                universities_group.name, "Institutions",
                "School1's parent group should be Institutions"
            );
        }
    }
}

// Test comprehensive report
#[tracing_test::traced_test]
#[tokio::test]
async fn test_comprehensive_report() {
    let admin = IdpAdmin::new(create_keycloak_config())
        .await
        .expect("Failed to create IdpAdmin");

    let report_result = admin.get_comprehensive_report().await;
    assert!(
        report_result.is_ok(),
        "Failed to get comprehensive report: {:?}",
        report_result.err()
    );

    let report = report_result.unwrap();
    assert!(
        !report.is_empty(),
        "Comprehensive report should not be empty"
    );

    // Find specific user types in the report
    let admin_user = report.iter().find(|u| u.username == "admin");
    assert!(admin_user.is_some(), "admin user not found in report");

    let team_captain = report.iter().find(|u| u.username == "captain1");
    assert!(team_captain.is_some(), "Team captain not found in report");

    let advisor = report.iter().find(|u| u.username == "advisor1");
    assert!(advisor.is_some(), "University advisor not found in report");

    let team_member = report.iter().find(|u| u.username == "member1");
    assert!(team_member.is_some(), "Team member not found in report");

    // Verify all users have their roles and groups populated
    for user in &report {
        // admin and global viewers don't have groups
        if ![
            "admin",
            "admin2",
            "viewer_global",
            "viewer_global2",
            "advisor1",
            "advisor2",
            "spec_school1",
            "spec_school2",
        ]
        .contains(&user.username.as_str())
        {
            assert!(
                user.team.is_some(),
                "User {} should have a team",
                user.username
            );
        }
    }

    // Test caching
    let cached_report = admin.get_comprehensive_report().await;
    assert!(cached_report.is_ok(), "Failed to get cached report");
    let cached = cached_report.unwrap();
    assert_eq!(
        report.len(),
        cached.len(),
        "Cached report should have same number of users"
    );
}

// Test cache invalidation for specific entities
#[tracing_test::traced_test]
#[tokio::test]
async fn test_cache_invalidation() {
    let admin = IdpAdmin::new(create_keycloak_config())
        .await
        .expect("Failed to create IdpAdmin");

    // First load data into cache
    let advisor_result = admin.find_users_by_username("advisor1").await.unwrap();
    let advisor = &advisor_result[0];

    // Load groups into cache
    let _ = admin.get_user_groups(advisor.id).await.unwrap();
    let _ = admin.get_groups(None).await.unwrap();

    // Now invalidate specific user cache
    admin.invalidate_user_cache(advisor.id).await;

    // Verify we can still get data (should reload from source)
    let advisor_groups_after = admin.get_user_groups(advisor.id).await;
    assert!(
        advisor_groups_after.is_ok(),
        "Should be able to reload user groups after cache invalidation"
    );

    // Test global cache invalidation
    admin.invalidate_caches();

    // Verify everything reloads correctly
    let _ = admin.get_users().await.unwrap();
    let _ = admin.get_groups(None).await.unwrap();
    let _ = admin.get_comprehensive_report().await.unwrap();
}
