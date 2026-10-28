use anyhow::Result;
use just_ingredients::db::*;
use sqlx::PgPool;

/// Database tests run against a fresh database created by `#[sqlx::test]` for each test,
/// so they can run in parallel and never touch the database `DATABASE_URL` points to.
/// They are ignored by default (no Postgres needed for `cargo test`); CI runs them with
/// `cargo test --test db_tests -- --include-ignored`.

#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn test_user_operations(pool: PgPool) -> Result<()> {
    init_database_schema(&pool).await?;
    test_user_operations_impl(&pool).await
}

async fn test_user_operations_impl(pool: &PgPool) -> Result<()> {
    let user = get_or_create_user(pool, 12345, Some("fr")).await?;
    assert_eq!(user.telegram_id, 12345);
    assert_eq!(user.language_code, std::sync::Arc::new("fr".to_string()));

    // Test getting existing user
    let user2 = get_or_create_user(pool, 12345, Some("en")).await?;
    assert_eq!(user2.id, user.id); // Should return same user
    assert_eq!(user2.language_code, std::sync::Arc::new("fr".to_string())); // Should keep original language

    // Test get_user_by_telegram_id
    let found_user = get_user_by_telegram_id(pool, 12345).await?;
    assert_eq!(found_user, Some(user.clone()));

    // Test get_user_by_id
    let found_user_by_id = get_user_by_id(pool, user.id).await?;
    assert_eq!(found_user_by_id, Some(user));

    Ok(())
}

#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn test_recipe_operations(pool: PgPool) -> Result<()> {
    init_database_schema(&pool).await?;
    test_recipe_operations_impl(&pool).await
}

async fn test_recipe_operations_impl(pool: &PgPool) -> Result<()> {
    let recipe_id = create_recipe(pool, 12345, "Test OCR content").await?;
    assert!(recipe_id > 0);

    // Read recipe
    let recipe = read_recipe(pool, 12345, recipe_id).await?;
    assert!(recipe.is_some());
    let recipe = recipe.unwrap();
    assert_eq!(recipe.telegram_id, 12345);
    assert_eq!(
        recipe.content,
        std::sync::Arc::new("Test OCR content".to_string())
    );

    // Update recipe
    let updated = update_recipe(pool, 12345, recipe_id, "Updated content").await?;
    assert!(updated);

    let updated_recipe = read_recipe(pool, 12345, recipe_id).await?;
    assert_eq!(
        updated_recipe.unwrap().content,
        std::sync::Arc::new("Updated content".to_string())
    );

    // Delete recipe
    let deleted = delete_recipe(pool, 12345, recipe_id).await?;
    assert!(deleted);

    let not_found = read_recipe(pool, 12345, recipe_id).await?;
    assert!(not_found.is_none());

    Ok(())
}

#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn test_ingredient_operations(pool: PgPool) -> Result<()> {
    init_database_schema(&pool).await?;
    test_ingredient_operations_impl(&pool).await
}

async fn test_ingredient_operations_impl(pool: &PgPool) -> Result<()> {
    let user = get_or_create_user(pool, 12345, None).await?;

    // Create recipe
    let recipe_id = create_recipe(pool, 12345, "flour 2 cups").await?;

    // Create ingredient
    let ingredient_id = create_ingredient(
        pool,
        user.id,
        Some(recipe_id),
        "flour",
        Some(2.0),
        Some("cups"),
        "flour 2 cups",
    )
    .await?;
    assert!(ingredient_id > 0);

    // Read ingredient
    let ingredient = read_ingredient(pool, 12345, ingredient_id).await?;
    assert!(ingredient.is_some());
    let ingredient = ingredient.unwrap();
    assert_eq!(ingredient.user_id, user.id);
    assert_eq!(ingredient.recipe_id, Some(recipe_id));
    assert_eq!(ingredient.name, "flour");
    assert_eq!(ingredient.quantity, Some(2.0));
    assert_eq!(ingredient.unit, Some("cups".to_string()));

    // Update ingredient
    let updated = update_ingredient(
        pool,
        12345,
        ingredient_id,
        Some("bread flour"),
        Some(3.0),
        Some("cups"),
    )
    .await?;
    assert!(updated);

    let updated_ingredient = read_ingredient(pool, 12345, ingredient_id).await?;
    assert_eq!(updated_ingredient.unwrap().name, "bread flour");

    // List ingredients by user
    let ingredients = list_ingredients_by_user(pool, user.id).await?;
    assert_eq!(ingredients.len(), 1);
    assert_eq!(ingredients[0].name, "bread flour");

    // Delete ingredient
    let deleted = delete_ingredient(pool, 12345, ingredient_id).await?;
    assert!(deleted);

    let not_found = read_ingredient(pool, 12345, ingredient_id).await?;
    assert!(not_found.is_none());

    Ok(())
}

#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn test_full_text_search(pool: PgPool) -> Result<()> {
    init_database_schema(&pool).await?;
    test_full_text_search_impl(&pool).await
}

async fn test_full_text_search_impl(pool: &PgPool) -> Result<()> {
    create_recipe(pool, 12345, "flour 2 cups sugar 1 cup").await?;
    create_recipe(pool, 12345, "butter 100 grams milk 250 ml").await?;
    create_recipe(pool, 67890, "chocolate 200 grams").await?;

    // Search for entries containing "flour"
    let results = search_recipes(pool, 12345, "flour").await?;
    assert_eq!(results.len(), 1);
    assert!(results[0].content.contains("flour"));

    // Search for entries containing "grams"
    let results = search_recipes(pool, 12345, "grams").await?;
    assert_eq!(results.len(), 1);
    assert!(results[0].content.contains("butter"));

    // Search for non-existent term
    let results = search_recipes(pool, 12345, "nonexistent").await?;
    assert_eq!(results.len(), 0);

    Ok(())
}

#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn test_get_user_recipes_paginated(pool: PgPool) -> Result<()> {
    init_database_schema(&pool).await?;
    test_get_user_recipes_paginated_impl(&pool).await
}

async fn test_get_user_recipes_paginated_impl(pool: &PgPool) -> Result<()> {
    // Create recipes with names
    let recipe1_id = create_recipe(pool, 12345, "flour 2 cups").await?;
    update_recipe_name(pool, 12345, recipe1_id, "Chocolate Cake").await?;

    let recipe2_id = create_recipe(pool, 12345, "butter 100g").await?;
    update_recipe_name(pool, 12345, recipe2_id, "Apple Pie").await?;

    let recipe3_id = create_recipe(pool, 12345, "sugar 1 cup").await?;
    update_recipe_name(pool, 12345, recipe3_id, "Banana Bread").await?;

    // Create recipe for different user
    let recipe4_id = create_recipe(pool, 67890, "milk 250ml").await?;
    update_recipe_name(pool, 67890, recipe4_id, "Pancakes").await?;

    // Test pagination: limit 2, offset 0
    let (recipes, total) = get_user_recipes_paginated(pool, 12345, 2, 0).await?;
    assert_eq!(total, 3);
    assert_eq!(recipes.len(), 2);
    assert!(recipes.contains(&"Apple Pie".to_string()));
    assert!(recipes.contains(&"Banana Bread".to_string()));

    // Test pagination: limit 2, offset 2
    let (recipes, total) = get_user_recipes_paginated(pool, 12345, 2, 2).await?;
    assert_eq!(total, 3);
    assert_eq!(recipes.len(), 1);
    assert_eq!(recipes[0], "Chocolate Cake");

    // Test with different user
    let (recipes, total) = get_user_recipes_paginated(pool, 67890, 10, 0).await?;
    assert_eq!(total, 1);
    assert_eq!(recipes.len(), 1);
    assert_eq!(recipes[0], "Pancakes");

    // Test with no recipes
    let (recipes, total) = get_user_recipes_paginated(pool, 99999, 10, 0).await?;
    assert_eq!(total, 0);
    assert_eq!(recipes.len(), 0);

    Ok(())
}

#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn test_save_recipe_is_all_or_nothing(pool: PgPool) -> Result<()> {
    init_database_schema(&pool).await?;
    let ingredients = [
        NewIngredient {
            name: "farine",
            quantity: Some(200.0),
            unit: Some("g"),
        },
        NewIngredient {
            name: "sel",
            quantity: None,
            unit: None,
        },
    ];
    let saved = save_recipe(
        &pool,
        12345,
        Some("fr"),
        "200 g farine\nsel",
        "Pain",
        &ingredients,
    )
    .await?;
    let recipe = read_recipe_with_name(&pool, 12345, saved.recipe_id)
        .await?
        .expect("saved");
    assert_eq!(
        recipe.recipe_name.as_deref().map(|n| n.as_str()),
        Some("Pain")
    );
    let stored = get_recipe_ingredients(&pool, 12345, saved.recipe_id).await?;
    assert_eq!(stored.len(), 2);
    assert_eq!(
        stored
            .iter()
            .find(|i| i.name == "sel")
            .and_then(|i| i.quantity),
        None
    );

    // The same user is reused, never duplicated
    let again = save_recipe(&pool, 12345, Some("en"), "x", "Tarte", &ingredients).await?;
    assert_eq!(again.user_id, saved.user_id);

    // An ingredient that cannot be stored (name over 255 characters) leaves nothing behind
    let too_long = "x".repeat(300);
    let failing = [
        NewIngredient {
            name: "beurre",
            quantity: Some(50.0),
            unit: Some("g"),
        },
        NewIngredient {
            name: &too_long,
            quantity: None,
            unit: None,
        },
    ];
    assert!(
        save_recipe(&pool, 12345, Some("fr"), "x", "Ratée", &failing)
            .await
            .is_err()
    );
    let (recipes, total) = get_user_recipes_newest_first(&pool, 12345, 10, 0).await?;
    assert_eq!(total, 2, "the failed save added no recipe: {recipes:?}");
    assert!(recipes.iter().all(|r| r.name != "Ratée"));
    Ok(())
}

#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn test_apply_ingredient_changes_writes_exact_values_in_one_go(pool: PgPool) -> Result<()> {
    init_database_schema(&pool).await?;
    let saved = save_recipe(
        &pool,
        555,
        Some("fr"),
        "x",
        "Gâteau",
        &[
            NewIngredient {
                name: "sucre",
                quantity: Some(1.0),
                unit: Some("cup"),
            },
            NewIngredient {
                name: "sel",
                quantity: Some(2.0),
                unit: Some("g"),
            },
            NewIngredient {
                name: "beurre",
                quantity: Some(50.0),
                unit: Some("g"),
            },
        ],
    )
    .await?;
    let before = get_recipe_ingredients(&pool, 555, saved.recipe_id).await?;
    let id = |name: &str| {
        before
            .iter()
            .find(|i| i.name == name)
            .map(|i| i.id)
            .expect(name)
    };

    // Another user's recipe: refused, nothing written
    let refused =
        apply_ingredient_changes(&pool, 999, saved.recipe_id, None, &[], &[], &[id("beurre")])
            .await;
    assert!(refused.is_err());
    assert_eq!(
        get_recipe_ingredients(&pool, 555, saved.recipe_id)
            .await?
            .len(),
        3
    );

    apply_ingredient_changes(
        &pool,
        555,
        saved.recipe_id,
        Some("fr"),
        &[
            IngredientUpdate {
                id: id("sucre"),
                ingredient: NewIngredient {
                    name: "sucre",
                    quantity: Some(0.5),
                    unit: Some("cup"),
                },
            },
            IngredientUpdate {
                id: id("sel"),
                ingredient: NewIngredient {
                    name: "sel",
                    quantity: None,
                    unit: None,
                },
            },
        ],
        &[NewIngredient {
            name: "lait",
            quantity: Some(200.0),
            unit: Some("ml"),
        }],
        &[id("beurre")],
    )
    .await?;
    let after = get_recipe_ingredients(&pool, 555, saved.recipe_id).await?;
    let find = |name: &str| after.iter().find(|i| i.name == name);
    assert_eq!(find("sucre").and_then(|i| i.quantity), Some(0.5));
    let salt = find("sel").expect("sel");
    assert_eq!(
        (salt.quantity, salt.unit.as_deref()),
        (None, None),
        "quantity and unit cleared"
    );
    assert!(find("lait").is_some());
    assert!(find("beurre").is_none());
    Ok(())
}

#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn test_delete_recipe_removes_its_ingredients(pool: PgPool) -> Result<()> {
    init_database_schema(&pool).await?;
    let ingredients = [NewIngredient {
        name: "farine",
        quantity: Some(1.0),
        unit: None,
    }];
    let saved = save_recipe(&pool, 777, None, "x", "Crêpes", &ingredients).await?;
    assert!(
        !delete_recipe(&pool, 999, saved.recipe_id).await?,
        "another user's recipe"
    );
    assert!(delete_recipe(&pool, 777, saved.recipe_id).await?);
    assert!(read_recipe_with_name(&pool, 777, saved.recipe_id)
        .await?
        .is_none());
    assert!(get_recipe_ingredients(&pool, 777, saved.recipe_id)
        .await?
        .is_empty());
    Ok(())
}

#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn test_get_user_recipes_newest_first(pool: PgPool) -> Result<()> {
    init_database_schema(&pool).await?;

    // Two recipes with the same name stay two entries; the unnamed one is left out
    let mut ids = Vec::new();
    for name in ["Tarte", "Crêpes", "Tarte"] {
        let id = create_recipe(&pool, 12345, "content").await?;
        update_recipe_name(&pool, 12345, id, name).await?;
        ids.push(id);
    }
    create_recipe(&pool, 12345, "unnamed").await?;
    let other = create_recipe(&pool, 67890, "content").await?;
    update_recipe_name(&pool, 67890, other, "Pancakes").await?;

    let (recipes, total) = get_user_recipes_newest_first(&pool, 12345, 2, 0).await?;
    assert_eq!(total, 3);
    let page: Vec<(i64, &str)> = recipes.iter().map(|r| (r.id, r.name.as_str())).collect();
    assert_eq!(page, vec![(ids[2], "Tarte"), (ids[1], "Crêpes")]);

    let (recipes, total) = get_user_recipes_newest_first(&pool, 12345, 2, 2).await?;
    assert_eq!(total, 3);
    assert_eq!(recipes.len(), 1);
    assert_eq!(recipes[0].id, ids[0]);

    let (recipes, total) = get_user_recipes_newest_first(&pool, 99999, 3, 0).await?;
    assert_eq!((recipes.len(), total), (0, 0));

    assert!(get_user_recipes_newest_first(&pool, 12345, 0, 0)
        .await
        .is_err());
    Ok(())
}

#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn test_get_recipes_by_name(pool: PgPool) -> Result<()> {
    init_database_schema(&pool).await?;
    test_get_recipes_by_name_impl(&pool).await
}

async fn test_get_recipes_by_name_impl(pool: &PgPool) -> Result<()> {
    // Create multiple recipes with the same name
    let recipe1_id = create_recipe(pool, 12345, "flour 2 cups sugar 1 cup").await?;
    update_recipe_name(pool, 12345, recipe1_id, "Chocolate Cake").await?;

    let recipe2_id = create_recipe(pool, 12345, "butter 100g eggs 2").await?;
    update_recipe_name(pool, 12345, recipe2_id, "Chocolate Cake").await?;

    let recipe3_id = create_recipe(pool, 12345, "milk 250ml vanilla 1 tsp").await?;
    update_recipe_name(pool, 12345, recipe3_id, "Vanilla Pudding").await?;

    // Create recipe with same name for different user
    let recipe4_id = create_recipe(pool, 67890, "flour 1 cup").await?;
    update_recipe_name(pool, 67890, recipe4_id, "Chocolate Cake").await?;

    // Test getting multiple recipes with same name
    let recipes = get_recipes_by_name(pool, 12345, "Chocolate Cake").await?;
    assert_eq!(recipes.len(), 2);

    // Verify the recipes are returned in descending creation order (most recent first)
    assert_eq!(recipes[0].id, recipe2_id); // Second recipe created (most recent)
    assert_eq!(recipes[1].id, recipe1_id); // First recipe created
    assert_eq!(
        recipes[0].recipe_name.as_ref().unwrap().as_ref(),
        "Chocolate Cake"
    );
    assert_eq!(
        recipes[1].recipe_name.as_ref().unwrap().as_ref(),
        "Chocolate Cake"
    );

    // Test getting single recipe
    let recipes = get_recipes_by_name(pool, 12345, "Vanilla Pudding").await?;
    assert_eq!(recipes.len(), 1);
    assert_eq!(recipes[0].id, recipe3_id);
    assert_eq!(
        recipes[0].recipe_name.as_ref().unwrap().as_ref(),
        "Vanilla Pudding"
    );

    // Test getting recipes for different user
    let recipes = get_recipes_by_name(pool, 67890, "Chocolate Cake").await?;
    assert_eq!(recipes.len(), 1);
    assert_eq!(recipes[0].id, recipe4_id);

    // Test getting non-existent recipe name
    let recipes = get_recipes_by_name(pool, 12345, "Non-existent Recipe").await?;
    assert_eq!(recipes.len(), 0);

    Ok(())
}

#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn test_has_duplicate_recipes(pool: PgPool) -> Result<()> {
    init_database_schema(&pool).await?;
    test_has_duplicate_recipes_impl(&pool).await
}

async fn test_has_duplicate_recipes_impl(pool: &PgPool) -> Result<()> {
    // Create multiple recipes with the same name
    let recipe1_id = create_recipe(pool, 12345, "flour 2 cups").await?;
    update_recipe_name(pool, 12345, recipe1_id, "Chocolate Cake").await?;

    let recipe2_id = create_recipe(pool, 12345, "butter 100g").await?;
    update_recipe_name(pool, 12345, recipe2_id, "Chocolate Cake").await?;

    // Create single recipe with different name
    let recipe3_id = create_recipe(pool, 12345, "milk 250ml").await?;
    update_recipe_name(pool, 12345, recipe3_id, "Vanilla Pudding").await?;

    // Test duplicate detection - should return true for "Chocolate Cake"
    let has_duplicates = has_duplicate_recipes(pool, 12345, "Chocolate Cake").await?;
    assert!(has_duplicates);

    // Test no duplicates - should return false for "Vanilla Pudding"
    let has_duplicates = has_duplicate_recipes(pool, 12345, "Vanilla Pudding").await?;
    assert!(!has_duplicates);

    // Test non-existent recipe name - should return false
    let has_duplicates = has_duplicate_recipes(pool, 12345, "Non-existent Recipe").await?;
    assert!(!has_duplicates);

    // Test with different user - should return false even if name exists for another user
    let has_duplicates = has_duplicate_recipes(pool, 67890, "Chocolate Cake").await?;
    assert!(!has_duplicates);

    Ok(())
}

// Ownership (IMP-002): recipe ids come from client-controlled callback data, so every
// recipe and ingredient query must be scoped by the owner. User B must get "not found"
// for user A's data, and user A must still be able to use it.
//
// Telegram ids are far from the sequential recipe/ingredient ids so that swapping the
// owner and id arguments cannot accidentally pass.
const OWNER_A: i64 = 111_111_111;
const OTHER_B: i64 = 222_222_222;

/// Create a named recipe with one ingredient for `telegram_id`.
/// Returns (recipe_id, ingredient_id).
async fn create_recipe_with_ingredient(
    pool: &PgPool,
    telegram_id: i64,
    recipe_name: &str,
) -> Result<(i64, i64)> {
    let user = get_or_create_user(pool, telegram_id, Some("en")).await?;
    let recipe_id = create_recipe(pool, telegram_id, "flour 2 cups").await?;
    assert!(update_recipe_name(pool, telegram_id, recipe_id, recipe_name).await?);
    let ingredient_id = create_ingredient(
        pool,
        user.id,
        Some(recipe_id),
        "flour",
        Some(2.0),
        Some("cups"),
        "flour 2 cups",
    )
    .await?;
    Ok((recipe_id, ingredient_id))
}

#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn test_recipe_reads_are_scoped_by_owner(pool: PgPool) -> Result<()> {
    init_database_schema(&pool).await?;
    test_recipe_reads_are_scoped_by_owner_impl(&pool).await
}

async fn test_recipe_reads_are_scoped_by_owner_impl(pool: &PgPool) -> Result<()> {
    let (recipe_id, _) = create_recipe_with_ingredient(pool, OWNER_A, "Secret Cake").await?;
    // B exists too, so "not found" is not just a missing user
    create_recipe_with_ingredient(pool, OTHER_B, "Other Cake").await?;

    // User B: A's recipe is not found
    assert!(read_recipe(pool, OTHER_B, recipe_id).await?.is_none());
    assert!(read_recipe_with_name(pool, OTHER_B, recipe_id)
        .await?
        .is_none());
    assert!(get_recipe_ingredients(pool, OTHER_B, recipe_id)
        .await?
        .is_empty());

    // User A: still sees the recipe and its ingredients
    let recipe = read_recipe(pool, OWNER_A, recipe_id)
        .await?
        .expect("owner should read own recipe");
    assert_eq!(recipe.telegram_id, OWNER_A);
    let recipe = read_recipe_with_name(pool, OWNER_A, recipe_id)
        .await?
        .expect("owner should read own recipe with name");
    assert_eq!(
        recipe.recipe_name.as_deref().map(String::as_str),
        Some("Secret Cake")
    );
    assert_eq!(
        get_recipe_ingredients(pool, OWNER_A, recipe_id)
            .await?
            .len(),
        1
    );

    Ok(())
}

#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn test_recipe_writes_are_scoped_by_owner(pool: PgPool) -> Result<()> {
    init_database_schema(&pool).await?;
    test_recipe_writes_are_scoped_by_owner_impl(&pool).await
}

async fn test_recipe_writes_are_scoped_by_owner_impl(pool: &PgPool) -> Result<()> {
    let (recipe_id, _) = create_recipe_with_ingredient(pool, OWNER_A, "Secret Cake").await?;
    get_or_create_user(pool, OTHER_B, Some("en")).await?;

    // User B: every write reports "not found" and changes nothing
    assert!(!update_recipe_name(pool, OTHER_B, recipe_id, "Stolen").await?);
    assert!(!update_recipe(pool, OTHER_B, recipe_id, "stolen content").await?);
    assert!(!delete_recipe(pool, OTHER_B, recipe_id).await?);

    let recipe = read_recipe_with_name(pool, OWNER_A, recipe_id)
        .await?
        .expect("B must not delete A's recipe");
    assert_eq!(
        recipe.recipe_name.as_deref().map(String::as_str),
        Some("Secret Cake")
    );
    assert_eq!(recipe.content.as_str(), "flour 2 cups");
    assert_eq!(
        get_recipe_ingredients(pool, OWNER_A, recipe_id)
            .await?
            .len(),
        1,
        "B's delete attempt must not remove A's ingredients"
    );

    // User A: can rename, update and delete
    assert!(update_recipe_name(pool, OWNER_A, recipe_id, "Renamed Cake").await?);
    assert!(update_recipe(pool, OWNER_A, recipe_id, "sugar 1 cup").await?);
    let recipe = read_recipe_with_name(pool, OWNER_A, recipe_id)
        .await?
        .expect("owner should read own recipe");
    assert_eq!(
        recipe.recipe_name.as_deref().map(String::as_str),
        Some("Renamed Cake")
    );
    assert_eq!(recipe.content.as_str(), "sugar 1 cup");

    assert!(delete_recipe(pool, OWNER_A, recipe_id).await?);
    assert!(read_recipe(pool, OWNER_A, recipe_id).await?.is_none());
    assert!(get_recipe_ingredients(pool, OWNER_A, recipe_id)
        .await?
        .is_empty());

    Ok(())
}

#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn test_ingredient_operations_are_scoped_by_owner(pool: PgPool) -> Result<()> {
    init_database_schema(&pool).await?;
    test_ingredient_operations_are_scoped_by_owner_impl(&pool).await
}

async fn test_ingredient_operations_are_scoped_by_owner_impl(pool: &PgPool) -> Result<()> {
    let (_, ingredient_id) = create_recipe_with_ingredient(pool, OWNER_A, "Secret Cake").await?;
    get_or_create_user(pool, OTHER_B, Some("en")).await?;

    // User B: A's ingredient is not found and cannot be changed
    assert!(read_ingredient(pool, OTHER_B, ingredient_id)
        .await?
        .is_none());
    assert!(
        !update_ingredient(
            pool,
            OTHER_B,
            ingredient_id,
            Some("salt"),
            Some(9.0),
            Some("kg")
        )
        .await?
    );
    assert!(!delete_ingredient(pool, OTHER_B, ingredient_id).await?);

    let ingredient = read_ingredient(pool, OWNER_A, ingredient_id)
        .await?
        .expect("B must not delete A's ingredient");
    assert_eq!(ingredient.name, "flour");
    assert_eq!(ingredient.quantity, Some(2.0));
    assert_eq!(ingredient.unit, Some("cups".to_string()));

    // User A: can update and delete
    assert!(
        update_ingredient(
            pool,
            OWNER_A,
            ingredient_id,
            Some("bread flour"),
            Some(3.0),
            None
        )
        .await?
    );
    let ingredient = read_ingredient(pool, OWNER_A, ingredient_id)
        .await?
        .expect("owner should read own ingredient");
    assert_eq!(ingredient.name, "bread flour");
    assert_eq!(ingredient.quantity, Some(3.0));

    assert!(delete_ingredient(pool, OWNER_A, ingredient_id).await?);
    assert!(read_ingredient(pool, OWNER_A, ingredient_id)
        .await?
        .is_none());

    Ok(())
}

#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn test_schema_validation(pool: PgPool) -> Result<()> {
    init_database_schema(&pool).await?;
    test_schema_validation_impl(&pool).await
}

async fn test_schema_validation_impl(pool: &PgPool) -> Result<()> {
    // Schema should be valid after initialization
    validate_database_schema(pool).await?;

    // Test that validation fails if a required table is missing
    sqlx::query("DROP TABLE IF EXISTS ingredients CASCADE")
        .execute(pool)
        .await?;

    let result = validate_database_schema(pool).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("ingredients"));

    // Recreate the table and test again. The migration is recorded as applied, so reset the
    // migration state first, otherwise init_database_schema has nothing to apply.
    sqlx::query("DROP TABLE IF EXISTS schema_migrations CASCADE")
        .execute(pool)
        .await?;
    init_database_schema(pool).await?;
    validate_database_schema(pool).await?;

    Ok(())
}

#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn test_migration_system(pool: PgPool) -> Result<()> {
    init_database_schema(&pool).await?;
    test_migration_system_impl(&pool).await
}

async fn test_migration_system_impl(pool: &PgPool) -> Result<()> {
    // Clean up any existing migration state
    sqlx::query("DROP TABLE IF EXISTS schema_migrations CASCADE")
        .execute(pool)
        .await?;
    sqlx::query("DROP TABLE IF EXISTS ingredients CASCADE")
        .execute(pool)
        .await?;
    sqlx::query("DROP TABLE IF EXISTS recipes CASCADE")
        .execute(pool)
        .await?;
    sqlx::query("DROP TABLE IF EXISTS users CASCADE")
        .execute(pool)
        .await?;

    // Test initial state - no migrations applied (the tracking table exists but is empty)
    migrations::init_migrations_table(pool).await?;
    let version = migrations::get_current_version(pool).await?;
    assert_eq!(version, 0);

    // Apply migrations
    migrations::apply_pending_migrations(pool).await?;

    // Check that migrations table was created and version is updated
    let version = migrations::get_current_version(pool).await?;
    assert_eq!(version, 1);

    // Verify that tables were created
    validate_database_schema(pool).await?;

    // Roll back with the down script of migration 1: version 0, tables removed
    migrations::rollback_to_version(pool, 0).await?;
    assert_eq!(migrations::get_current_version(pool).await?, 0);
    assert!(validate_database_schema(pool).await.is_err());

    Ok(())
}

#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn test_schema_validation_details(pool: PgPool) -> Result<()> {
    init_database_schema(&pool).await?;
    test_schema_validation_details_impl(&pool).await
}

async fn test_schema_validation_details_impl(pool: &PgPool) -> Result<()> {
    // Test that all expected columns exist with correct types

    // Check users table columns
    let user_columns = vec![
        ("id", "bigint"),
        ("telegram_id", "bigint"),
        ("language_code", "character varying"),
        ("created_at", "timestamp with time zone"),
        ("updated_at", "timestamp with time zone"),
    ];

    for (col_name, _expected_type) in user_columns {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM information_schema.columns
             WHERE table_name = 'users' AND column_name = $1 AND table_schema = 'public')",
        )
        .bind(col_name)
        .fetch_one(pool)
        .await?;
        assert!(exists, "Column {} should exist in users table", col_name);
    }

    // Check ingredients table has raw_text column
    let has_raw_text: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM information_schema.columns
         WHERE table_name = 'ingredients' AND column_name = 'raw_text' AND table_schema = 'public')"
    )
    .fetch_one(pool)
    .await?;
    assert!(
        has_raw_text,
        "raw_text column should exist in ingredients table"
    );

    // Check that required indexes exist
    let indexes = vec![
        ("recipes_content_tsv_idx", "recipes"),
        ("ingredients_user_id_idx", "ingredients"),
        ("ingredients_recipe_id_idx", "ingredients"),
    ];

    for (index_name, table_name) in indexes {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM pg_indexes WHERE tablename = $1 AND indexname = $2)",
        )
        .bind(table_name)
        .bind(index_name)
        .fetch_one(pool)
        .await?;
        assert!(
            exists,
            "Index {} should exist on table {}",
            index_name, table_name
        );
    }

    Ok(())
}

#[test]
fn test_split_sql_statements_single_statement() {
    let sql = "CREATE TABLE test (id INT);";
    let statements = just_ingredients::db::migrations::split_sql_statements(sql);
    assert_eq!(
        statements,
        Ok(vec!["CREATE TABLE test (id INT);".to_string()])
    );
}

#[test]
fn test_split_sql_statements_multiple_statements() {
    let sql = "CREATE TABLE test (id INT); INSERT INTO test VALUES (1);";
    let statements = just_ingredients::db::migrations::split_sql_statements(sql);
    assert_eq!(
        statements,
        Ok(vec![
            "CREATE TABLE test (id INT);".to_string(),
            "INSERT INTO test VALUES (1);".to_string()
        ])
    );
}

#[test]
fn test_split_sql_statements_with_string_literals() {
    let sql = "INSERT INTO test VALUES ('hello;world'); CREATE TABLE test2 (id INT);";
    let statements = just_ingredients::db::migrations::split_sql_statements(sql);
    assert_eq!(
        statements,
        Ok(vec![
            "INSERT INTO test VALUES ('hello;world');".to_string(),
            "CREATE TABLE test2 (id INT);".to_string()
        ])
    );
}

#[test]
fn test_split_sql_statements_with_double_quotes() {
    let sql = r#"INSERT INTO test VALUES ("hello;world"); CREATE TABLE test2 (id INT);"#;
    let statements = just_ingredients::db::migrations::split_sql_statements(sql);
    assert_eq!(
        statements,
        Ok(vec![
            r#"INSERT INTO test VALUES ("hello;world");"#.to_string(),
            "CREATE TABLE test2 (id INT);".to_string()
        ])
    );
}

#[test]
fn test_split_sql_statements_with_comments() {
    let sql = r#"
        -- This is a comment
        CREATE TABLE test (id INT);
        -- Another comment
        INSERT INTO test VALUES (1);
    "#;
    let statements = just_ingredients::db::migrations::split_sql_statements(sql);
    assert_eq!(
        statements,
        Ok(vec![
            "-- This is a comment\n        CREATE TABLE test (id INT);".to_string(),
            "-- Another comment\n        INSERT INTO test VALUES (1);".to_string()
        ])
    );
}

#[test]
fn test_split_sql_statements_mixed_strings_and_comments() {
    let sql = r#"
        -- Create table
        CREATE TABLE test (
            id INT,
            name VARCHAR(100) DEFAULT 'test;value'
        );
        -- Insert data
        INSERT INTO test VALUES (1, 'hello;world');
    "#;
    let statements = just_ingredients::db::migrations::split_sql_statements(sql);
    match statements {
        Ok(stmts) => {
            assert_eq!(stmts.len(), 2);
            assert!(stmts[0].contains("CREATE TABLE"));
            assert!(stmts[1].contains("INSERT INTO"));
        }
        Err(e) => panic!("Failed to split SQL: {}", e),
    }
}

#[test]
fn test_split_sql_statements_empty_input() {
    let sql = "";
    let statements = just_ingredients::db::migrations::split_sql_statements(sql);
    assert_eq!(statements, Ok(Vec::<String>::new()));
}

#[test]
fn test_split_sql_statements_only_whitespace() {
    let sql = "   \n\t   ";
    let statements = just_ingredients::db::migrations::split_sql_statements(sql);
    assert_eq!(statements, Ok(Vec::<String>::new()));
}

#[test]
fn test_split_sql_statements_no_trailing_semicolon() {
    let sql = "CREATE TABLE test (id INT)";
    let statements = just_ingredients::db::migrations::split_sql_statements(sql);
    assert_eq!(
        statements,
        Ok(vec!["CREATE TABLE test (id INT)".to_string()])
    );
}

#[test]
fn test_split_sql_statements_complex_migration_sql() {
    let sql = r#"
        -- Create users table
        CREATE TABLE IF NOT EXISTS users (
            id BIGSERIAL PRIMARY KEY,
            telegram_id BIGINT UNIQUE NOT NULL,
            language_code VARCHAR(10) DEFAULT 'en',
            created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
            updated_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
        );

        -- Create recipes table
        CREATE TABLE IF NOT EXISTS recipes (
            id BIGSERIAL PRIMARY KEY,
            telegram_id BIGINT NOT NULL,
            content TEXT NOT NULL,
            recipe_name VARCHAR(255),
            created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
            content_tsv tsvector GENERATED ALWAYS AS (to_tsvector('english', content)) STORED
        );

        -- Create indexes
        CREATE INDEX IF NOT EXISTS recipes_content_tsv_idx ON recipes USING GIN (content_tsv);
    "#;
    let statements = just_ingredients::db::migrations::split_sql_statements(sql);
    match statements {
        Ok(stmts) => {
            assert_eq!(stmts.len(), 3);
            assert!(stmts[0].contains("CREATE TABLE IF NOT EXISTS users"));
            assert!(stmts[1].contains("CREATE TABLE IF NOT EXISTS recipes"));
            assert!(stmts[2].contains("CREATE INDEX IF NOT EXISTS recipes_content_tsv_idx"));
        }
        Err(e) => panic!("Failed to split SQL: {}", e),
    }
}

#[test]
fn test_split_sql_statements_semicolon_in_string_and_comment() {
    let sql = r#"
        -- This comment has ; in it
        INSERT INTO test VALUES ('value;with;semicolons');
        -- Another ; comment
        UPDATE test SET value = 'new;value';
    "#;
    let statements = just_ingredients::db::migrations::split_sql_statements(sql);
    match statements {
        Ok(stmts) => {
            assert_eq!(stmts.len(), 2);
            assert!(stmts[0].contains("INSERT INTO"));
            assert!(stmts[1].contains("UPDATE"));
        }
        Err(e) => panic!("Failed to split SQL: {}", e),
    }
}

#[sqlx::test(migrations = false)]
#[ignore = "requires Postgres: set DATABASE_URL and run with --include-ignored"]
async fn test_long_recipe_names_resolve_by_fingerprint_for_their_owner(pool: PgPool) -> Result<()> {
    use just_ingredients::bot::recipe_selection::{
        recipe_selection_callback, resolve_recipe_selection, MAX_CALLBACK_DATA_BYTES,
    };
    init_database_schema(&pool).await?;
    let long_name =
        "https://www.marmiton.org/recettes/recette_gateau-au-chocolat-fondant_166352.aspx";
    let recipe_id = create_recipe(&pool, 111, "1 cup flour").await?;
    update_recipe_name(&pool, 111, recipe_id, long_name).await?;

    let data = recipe_selection_callback(long_name);
    assert!(data.len() <= MAX_CALLBACK_DATA_BYTES);
    assert_eq!(
        resolve_recipe_selection(&pool, 111, &data)
            .await?
            .as_deref(),
        Some(long_name)
    );
    assert_eq!(
        resolve_recipe_selection(&pool, 222, &data).await?,
        None,
        "another user's fingerprint does not resolve"
    );
    Ok(())
}
