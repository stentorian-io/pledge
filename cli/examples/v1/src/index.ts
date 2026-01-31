/**
 * Test fixture package v1.0.0 - Baseline version
 *
 * This file contains exports that will be compared against v2 versions
 * to test breaking change detection.
 */

// ============================================================================
// Types and Interfaces
// ============================================================================

/**
 * User object returned by user-related functions
 */
export interface User {
  id: number;
  name: string;
  email: string;
  createdAt: Date;
}

/**
 * Configuration interface for the service
 * Note: timeout is OPTIONAL in v1
 */
export interface Config {
  apiKey: string;
  timeout?: number;
  baseUrl?: string;
}

/**
 * Status union type
 * Values: 'active' | 'inactive'
 */
export type Status = 'active' | 'inactive';

/**
 * Role type for user permissions
 */
export type Role = 'admin' | 'user' | 'guest';

// ============================================================================
// Constants
// ============================================================================

/**
 * Library version - will be REMOVED in v2-breaking
 */
export const VERSION: string = '1.0.0';

/**
 * Default timeout value in milliseconds
 */
export const DEFAULT_TIMEOUT: number = 5000;

// ============================================================================
// Functions
// ============================================================================

/**
 * Creates a new user
 *
 * @param name - User's display name
 * @param email - User's email address
 * @returns The created user object
 *
 * Note: v2-breaking adds a required 'role' parameter
 * Note: v2-compatible adds an optional 'role' parameter
 */
export function createUser(name: string, email: string): User {
  return {
    id: Math.floor(Math.random() * 10000),
    name,
    email,
    createdAt: new Date(),
  };
}

/**
 * Validates an email address format
 *
 * @param email - Email string to validate
 * @returns true if valid, false otherwise
 */
export function validateEmail(email: string): boolean {
  const emailRegex = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;
  return emailRegex.test(email);
}

/**
 * Formats a user object for display
 *
 * @param user - User object to format
 * @returns Formatted string representation
 */
export function formatUserDisplay(user: User): string {
  return `${user.name} <${user.email}>`;
}

// ============================================================================
// Classes
// ============================================================================

/**
 * Service for managing users
 *
 * Note: getUser param type changes from number to string in v2-breaking
 */
export class UserService {
  private users: Map<number, User> = new Map();
  private config: Config;

  constructor(config: Config) {
    this.config = config;
  }

  /**
   * Retrieves a user by their ID
   *
   * @param id - User's numeric ID (changes to string in v2-breaking)
   * @returns The user if found, undefined otherwise
   */
  getUser(id: number): User | undefined {
    return this.users.get(id);
  }

  /**
   * Adds a new user to the service
   *
   * @param user - User object to add
   */
  addUser(user: User): void {
    this.users.set(user.id, user);
  }

  /**
   * Lists all users
   *
   * @returns Array of all users
   */
  listUsers(): User[] {
    return Array.from(this.users.values());
  }

  /**
   * Gets the current configuration
   *
   * @returns The service configuration
   */
  getConfig(): Config {
    return this.config;
  }
}

/**
 * Logger class for debugging
 */
export class Logger {
  private prefix: string;

  constructor(prefix: string) {
    this.prefix = prefix;
  }

  log(message: string): void {
    console.log(`[${this.prefix}] ${message}`);
  }

  error(message: string): void {
    console.error(`[${this.prefix}] ERROR: ${message}`);
  }
}

// ============================================================================
// Abstract Classes
// ============================================================================

/**
 * Base repository for data access
 */
export abstract class BaseRepository<T> {
  abstract findById(id: number): T | undefined;
  abstract save(item: T): void;
  abstract delete(id: number): boolean;
}

// ============================================================================
// Enums
// ============================================================================

/**
 * HTTP status codes
 */
export enum HttpStatus {
  OK = 200,
  CREATED = 201,
  BAD_REQUEST = 400,
  NOT_FOUND = 404,
  INTERNAL_ERROR = 500,
}

// ============================================================================
// Type Aliases and Utility Types
// ============================================================================

/**
 * Callback type for async operations
 */
export type AsyncCallback<T> = (error: Error | null, result?: T) => void;

/**
 * Result type for operations that can fail
 */
export type Result<T, E = Error> =
  | { success: true; data: T }
  | { success: false; error: E };
