use clap::{Parser, Subcommand, Args};
use sea_orm::{Database, DatabaseConnection, EntityTrait, ActiveValue::Set};
use std::env;
use dotenv::dotenv;
use uuid::Uuid;
use panel::entities::users;
use panel::services::auth::hash_password;

#[derive(Parser)]
#[command(name = "yunexal-cli")]
#[command(about = "CLI tool for Yunexal Panel management")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new user
    CreateUser(CreateUserArgs),
    /// Setup the database (run migrations)
    Setup,
}

#[derive(Args)]
struct CreateUserArgs {
    #[arg(short, long)]
    username: String,
    
    #[arg(short, long)]
    email: String,
    
    #[arg(short, long)]
    password: String,
    
    #[arg(long, default_value_t = false)]
    admin: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok();
    
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let db = Database::connect(&database_url).await?;

    let cli = Cli::parse();

    match &cli.command {
        Commands::CreateUser(args) => {
            create_user(&db, args).await?;
        }
        Commands::Setup => {
            println!("Running migrations...");
            // In a real scenario, you might call migration logic here
            // For now, we assume the main app handles migration on startup
            println!("Setup complete (migrations rely on main app startup).");
        }
    }

    Ok(())
}

async fn create_user(db: &DatabaseConnection, args: &CreateUserArgs) -> Result<(), Box<dyn std::error::Error>> {
    let password_hash = hash_password(&args.password)?;
    let user_id = Uuid::new_v4();

    let user = users::ActiveModel {
        id: Set(user_id),
        username: Set(args.username.clone()),
        email: Set(args.email.clone()),
        password_hash: Set(password_hash),
        role: Set(if args.admin { "admin".to_string() } else { "user".to_string() }), 
        permissions: Set(None),
        created_at: Set(chrono::Utc::now()),
    };

    users::Entity::insert(user).exec(db).await?;
    println!("User created successfully: {} ({})", args.username, user_id);
    Ok(())
}
